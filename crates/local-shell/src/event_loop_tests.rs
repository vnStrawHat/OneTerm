//! Tests for the local-shell event loop: the notifier's queue policy, the
//! child-exit lifecycle, and the loop itself driven through an in-memory
//! loopback PTY instead of a real shell (TEST-02).

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use alacritty_terminal::tty::{ChildEvent, EventedReadWrite};
use oneterm_terminal::{
    ClipboardOrigin, GridSize, OscRouter, SessionEvent, SessionEventSink, SharedSessionState,
};

use super::*;
use crate::transport::LocalTransport;

fn notifier(
    capacity: usize,
) -> (
    ShellNotifier,
    mpsc::Receiver<Cow<'static, [u8]>>,
    std::sync::Arc<ShellControl>,
) {
    let poller = std::sync::Arc::new(Poller::new().unwrap());
    let control = std::sync::Arc::new(ShellControl::default());
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let notifier = ShellNotifier {
        sender,
        poller,
        control: control.clone(),
    };
    (notifier, receiver, control)
}

#[test]
fn input_queue_is_bounded_by_messages_and_bytes() {
    let (notifier, receiver, control) = notifier(1);
    notifier.send(ShellMsg::Input(Cow::Owned(vec![1]))).unwrap();
    let error = notifier
        .send(ShellMsg::Input(Cow::Owned(vec![2])))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(control.queued_input_bytes.load(Ordering::Acquire), 1);
    assert_eq!(receiver.try_recv().unwrap().as_ref(), [1]);
}

#[test]
fn aggregate_local_input_bytes_are_bounded() {
    let (notifier, receiver, control) = notifier(2);
    notifier
        .send(ShellMsg::Input(Cow::Owned(vec![
            0;
            LOCAL_COMMAND_BYTE_BUDGET
        ])))
        .unwrap();
    let error = notifier
        .send(ShellMsg::Input(Cow::Owned(vec![1])))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(receiver.try_iter().count(), 1);
    assert_eq!(
        control.queued_input_bytes.load(Ordering::Acquire),
        LOCAL_COMMAND_BYTE_BUDGET
    );
}

#[test]
fn local_input_queue_preserves_fifo_order() {
    let (notifier, receiver, _control) = notifier(2);
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"first")))
        .unwrap();
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"second")))
        .unwrap();
    assert_eq!(receiver.try_recv().unwrap().as_ref(), b"first");
    assert_eq!(receiver.try_recv().unwrap().as_ref(), b"second");
}

#[test]
fn resize_is_latest_value_and_shutdown_is_immediate() {
    let (notifier, receiver, control) = notifier(1);
    let first = WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 0,
        cell_height: 0,
    };
    let latest = WindowSize {
        num_lines: 40,
        num_cols: 120,
        cell_width: 0,
        cell_height: 0,
    };
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"queue is full")))
        .unwrap();
    notifier.send(ShellMsg::Resize(first)).unwrap();
    notifier.send(ShellMsg::Resize(latest)).unwrap();
    let pending = control.pending_resize.lock().unwrap().take().unwrap();
    assert_eq!((pending.num_lines, pending.num_cols), (40, 120));
    assert_eq!(receiver.try_iter().count(), 1);

    notifier.send(ShellMsg::Shutdown).unwrap();
    assert!(control.shutdown.load(Ordering::Acquire));
}

// ── Shared fixtures ──────────────────────────────────────────────────────

fn router_and_events() -> (
    LocalListener,
    async_channel::Receiver<SessionEvent>,
    oneterm_terminal::SharedState,
) {
    let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(64);
    let state = SharedSessionState::new_alive();
    let listener = OscRouter::new(
        LocalTransport::new(),
        SessionEventSink::new(event_tx),
        state.clone(),
        ClipboardOrigin::Local,
    );
    (listener, event_rx, state)
}

fn drain(events: &async_channel::Receiver<SessionEvent>) -> Vec<SessionEvent> {
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    predicate()
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code << 8)
}

#[cfg(windows)]
fn exit_status(code: u32) -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;

    std::process::ExitStatus::from_raw(code)
}

/// TEST-10 / CORR-05: an `Exited(None)` child event (Windows watcher could not
/// read the exit code) must still end the session: `alive` is cleared and both
/// `Exited` and `Closed` reach the UI.
#[test]
fn child_exit_without_status_still_ends_the_session() {
    let (listener, event_rx, state) = router_and_events();
    let pump = TerminalPump::new(listener);

    publish_child_exit(&pump, None);

    assert!(!state.alive());
    assert_eq!(state.exit_code(), None);
    assert_eq!(
        drain(&event_rx),
        vec![
            SessionEvent::Exited(None),
            SessionEvent::Output,
            SessionEvent::Closed
        ]
    );
}

/// A child exit with a status records the code and also emits `Closed`.
#[test]
fn child_exit_with_status_records_code_and_closes() {
    let (listener, event_rx, state) = router_and_events();
    let pump = TerminalPump::new(listener);

    publish_child_exit(&pump, Some(exit_status(3)));

    assert!(!state.alive());
    assert_eq!(state.exit_code(), Some(3));
    let events = drain(&event_rx);
    assert_eq!(events.first(), Some(&SessionEvent::Exited(Some(3))));
    assert_eq!(events.last(), Some(&SessionEvent::Closed));
}

// ── In-memory PTY: drives `ShellEventLoop::run` without a shell ──────────

fn tcp_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let client = TcpStream::connect(addr).unwrap();
    let (server, _) = listener.accept().unwrap();
    client.set_nodelay(true).unwrap();
    server.set_nodelay(true).unwrap();
    (client, server)
}

/// Loop-side half of the fake PTY: two loopback sockets (I/O + child signal).
struct LoopbackPty {
    io: TcpStream,
    child_signal: TcpStream,
    child_events: mpsc::Receiver<ChildEvent>,
    resizes: Arc<Mutex<Vec<WindowSize>>>,
}

/// Test-side half: the "shell" end of the fake PTY.
struct LoopbackPeer {
    io: TcpStream,
    child_signal: TcpStream,
    child_events: mpsc::Sender<ChildEvent>,
    resizes: Arc<Mutex<Vec<WindowSize>>>,
}

impl LoopbackPeer {
    /// Emit shell output toward the terminal.
    fn output(&mut self, bytes: &[u8]) {
        self.io.write_all(bytes).unwrap();
        self.io.flush().unwrap();
    }

    /// Read what the terminal wrote to the "shell".
    fn read_input(&mut self, expected_len: usize) -> Vec<u8> {
        let mut out = vec![0u8; expected_len];
        self.io
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        self.io.read_exact(&mut out).unwrap();
        out
    }

    /// Simulate child exit: queue the event and wake the loop on token 1.
    fn exit(&mut self, status: Option<std::process::ExitStatus>) {
        self.child_events.send(ChildEvent::Exited(status)).unwrap();
        self.child_signal.write_all(&[1]).unwrap();
        self.child_signal.flush().unwrap();
    }
}

fn loopback_pty() -> (LoopbackPty, LoopbackPeer) {
    let (io_loop, io_peer) = tcp_pair();
    let (sig_loop, sig_peer) = tcp_pair();
    io_loop.set_nonblocking(true).unwrap();
    sig_loop.set_nonblocking(true).unwrap();
    let (child_tx, child_rx) = mpsc::channel();
    let resizes = Arc::new(Mutex::new(Vec::new()));
    (
        LoopbackPty {
            io: io_loop,
            child_signal: sig_loop,
            child_events: child_rx,
            resizes: resizes.clone(),
        },
        LoopbackPeer {
            io: io_peer,
            child_signal: sig_peer,
            child_events: child_tx,
            resizes,
        },
    )
}

impl EventedReadWrite for LoopbackPty {
    type Reader = TcpStream;
    type Writer = TcpStream;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: PollEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = 0;
        unsafe {
            poll.add_with_mode(&self.io, interest, mode)?;
            poll.add_with_mode(
                &self.child_signal,
                PollEvent::readable(PTY_CHILD_EVENT_TOKEN),
                PollMode::Level,
            )
        }
    }

    fn reregister(
        &mut self,
        poll: &Arc<Poller>,
        mut interest: PollEvent,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = 0;
        poll.modify_with_mode(&self.io, interest, mode)?;
        poll.modify_with_mode(
            &self.child_signal,
            PollEvent::readable(PTY_CHILD_EVENT_TOKEN),
            PollMode::Level,
        )
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        poll.delete(&self.io)?;
        poll.delete(&self.child_signal)
    }

    fn reader(&mut self) -> &mut TcpStream {
        &mut self.io
    }

    fn writer(&mut self) -> &mut TcpStream {
        &mut self.io
    }
}

impl EventedPty for LoopbackPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        let mut byte = [0u8; 1];
        let _ = self.child_signal.read(&mut byte);
        self.child_events.try_recv().ok()
    }
}

impl OnResize for LoopbackPty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.resizes.lock().unwrap().push(window_size);
    }
}

struct RunningLoop {
    term: Arc<FairMutex<Term<LocalListener>>>,
    notifier: ShellNotifier,
    events: async_channel::Receiver<SessionEvent>,
    state: oneterm_terminal::SharedState,
    join: Option<std::thread::JoinHandle<()>>,
}

impl RunningLoop {
    fn screen_text(&self) -> String {
        let term = self.term.lock();
        let mut text = String::new();
        for line in 0..term.screen_lines() {
            for col in 0..term.columns() {
                let point = alacritty_terminal::index::Point::new(
                    alacritty_terminal::index::Line(line as i32),
                    alacritty_terminal::index::Column(col),
                );
                text.push(term.grid()[point].c);
            }
        }
        text
    }

    fn join(&mut self) {
        if let Some(join) = self.join.take() {
            join.join().unwrap();
        }
    }
}

impl Drop for RunningLoop {
    fn drop(&mut self) {
        let _ = self.notifier.send(ShellMsg::Shutdown);
        self.join();
    }
}

fn start_loop() -> (RunningLoop, LoopbackPeer) {
    let (pty, peer) = loopback_pty();
    let (listener, events, state) = router_and_events();
    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &GridSize {
            cols: 80,
            lines: 24,
        },
        listener.clone(),
    )));
    let (mut event_loop, notifier) = ShellEventLoop::new(pty, term.clone(), listener.clone())
        .expect("event loop over loopback pty");
    listener.transport().set_notifier(notifier.clone());
    let join = std::thread::Builder::new()
        .name("loopback PTY owner".into())
        .spawn(move || event_loop.run())
        .unwrap();
    (
        RunningLoop {
            term,
            notifier,
            events,
            state,
            join: Some(join),
        },
        peer,
    )
}

#[test]
fn loop_parses_pty_output_into_the_terminal_and_signals_repaint() {
    let (running, mut peer) = start_loop();
    peer.output(b"\x1b]2;title-from-pty\x07hello loopback\r\n");
    assert!(
        wait_until(Duration::from_secs(5), || running
            .screen_text()
            .contains("hello loopback")),
        "output did not reach the terminal grid"
    );
    assert!(wait_until(Duration::from_secs(2), || {
        running.state.title().as_deref() == Some("title-from-pty")
    }));
    let events = drain(&running.events);
    assert!(events.contains(&SessionEvent::Title("title-from-pty".into())));
    assert!(events.contains(&SessionEvent::Output));
    assert!(running.state.absolute_line_count() >= 24);
}

#[test]
fn loop_writes_queued_input_to_the_pty_in_order() {
    let (running, mut peer) = start_loop();
    running
        .notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"first ")))
        .unwrap();
    running
        .notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"second")))
        .unwrap();
    assert_eq!(peer.read_input(12), b"first second");
}

#[test]
fn loop_answers_color_queries_through_the_pty() {
    let (running, mut peer) = start_loop();
    peer.output(b"\x1b]11;rgb:1111/2222/3333\x07\x1b]11;?\x07");
    let reply = peer.read_input(24);
    assert!(
        reply.starts_with(b"\x1b]11;rgb:1111/2222/3333"),
        "{:?}",
        String::from_utf8_lossy(&reply)
    );
    drop(running);
}

#[test]
fn loop_applies_latest_resize_to_the_pty() {
    let (running, peer) = start_loop();
    let size = |lines, cols| WindowSize {
        num_lines: lines,
        num_cols: cols,
        cell_width: 0,
        cell_height: 0,
    };
    running
        .notifier
        .send(ShellMsg::Resize(size(30, 100)))
        .unwrap();
    assert!(wait_until(Duration::from_secs(2), || {
        !peer.resizes.lock().unwrap().is_empty()
    }));
    let applied = peer.resizes.lock().unwrap().last().copied().unwrap();
    assert_eq!((applied.num_lines, applied.num_cols), (30, 100));
}

#[test]
fn loop_child_exit_ends_the_session_and_stops_the_thread() {
    let (mut running, mut peer) = start_loop();
    peer.exit(Some(exit_status(0)));
    assert!(wait_until(Duration::from_secs(5), || !running
        .state
        .alive()));
    running.join();
    let events = drain(&running.events);
    assert!(
        events.contains(&SessionEvent::Exited(Some(0))),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&SessionEvent::Closed));
}

#[test]
fn loop_shutdown_stops_the_thread_without_lifecycle_events() {
    let (mut running, _peer) = start_loop();
    running.notifier.send(ShellMsg::Shutdown).unwrap();
    running.join();
    let events = drain(&running.events);
    assert!(!events.contains(&SessionEvent::Closed));
    // The transport reports closed once shutdown was requested.
    let error = running
        .notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"late")))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}
