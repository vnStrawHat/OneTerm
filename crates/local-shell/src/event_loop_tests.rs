//! Tests for the local-shell event loop: the notifier's queue policy, the
//! child-exit lifecycle, and the loop itself driven through an in-memory
//! loopback PTY instead of a real shell (TEST-02).

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use oneterm_pty::{ChildEvent, EventedReadWrite};
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
        rows: 24,
        cols: 80,
        cell_width: 0,
        cell_height: 0,
    };
    let latest = WindowSize {
        rows: 40,
        cols: 120,
        cell_width: 0,
        cell_height: 0,
    };
    notifier
        .send(ShellMsg::Input(Cow::Borrowed(b"queue is full")))
        .unwrap();
    notifier.send(ShellMsg::Resize(first)).unwrap();
    notifier.send(ShellMsg::Resize(latest)).unwrap();
    let pending = control.pending_resize.lock().unwrap().take().unwrap();
    assert_eq!((pending.rows, pending.cols), (40, 120));
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
        interest.key = PTY_READ_WRITE_TOKEN;
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
        interest.key = PTY_READ_WRITE_TOKEN;
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
    fn on_resize(&mut self, window_size: WindowSize) -> io::Result<()> {
        self.resizes.lock().unwrap().push(window_size);
        Ok(())
    }
}

struct RunningLoop {
    term: oneterm_terminal::SharedTerminal,
    notifier: ShellNotifier,
    events: async_channel::Receiver<SessionEvent>,
    state: oneterm_terminal::SharedState,
    join: Option<std::thread::JoinHandle<()>>,
}

impl RunningLoop {
    fn screen_text(&self) -> String {
        let term = self.term.lock();
        let screen = term.screen();
        let graphemes = &term.interner().graphemes;
        let mut text = String::new();
        for index in 0..screen.rows() {
            for cell in screen.row(screen.screen_top() + u64::from(index)).cells() {
                text.push(cell.text_char(graphemes));
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
    let term = oneterm_terminal::new_shared_terminal(
        GridSize {
            cols: 80,
            lines: 24,
        },
        oneterm_terminal::DEFAULT_SCROLLBACK_LINES,
    );
    let poll = std::sync::Arc::new(polling::Poller::new().expect("poller"));
    let (mut event_loop, notifier) = ShellEventLoop::new(pty, term.clone(), listener.clone(), poll);
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
    let size = |rows, cols| WindowSize {
        rows,
        cols,
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
    assert_eq!((applied.rows, applied.cols), (30, 100));
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

/// How long a frame may wait for the engine while the pump floods.
///
/// Not a stopwatch. The property is "**one batch**, not the whole flood", and a
/// batch here is whatever the socket buffer held, parsed at `test` profile
/// opt-level 0: measured worst-of-five at 86 / 108 / 125 ms over three runs,
/// against `US-0082`'s 157 µs for a 4 KiB in-process chunk. The failing side is
/// not slower, it never arrives — with the flood running, a loop that ignores
/// the demand holds the engine until the producer stops.
const HANDOVER_BOUND: Duration = Duration::from_millis(250);

/// Frames taken while the pump floods. One acquisition could be luck; the
/// assertion is on the worst of them.
const FRAMES: u32 = 5;

/// Stops the flood on the way out, including while unwinding: `RunningLoop`'s
/// drop joins the owner thread, and a thread still reading a fed pipe never
/// returns to `poll.wait` to see the shutdown flag.
struct StopFlood(Arc<AtomicBool>);

impl Drop for StopFlood {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// `US-0083`: a pump that reads until the pipe is empty must hand the engine to
/// a waiting frame at its next chunk boundary instead.
///
/// The `US-0082` verifier measured both outcomes on this shape — honoured: one
/// batch, 157 µs; ignored: 3 800 batches, 354 ms for a *bounded* 4 MiB flood.
/// The flood below does not stop while the frames are taken, so a loop that
/// ignores the demand does not hand the lock over at all: a fair mutex cannot
/// help a waiter that never sees an unlock.
#[test]
fn a_flooding_loop_hands_the_engine_to_a_waiting_frame() {
    let (running, mut peer) = start_loop();
    let stop = Arc::new(AtomicBool::new(false));
    let _stop_flood = StopFlood(Arc::clone(&stop));

    // The UI keeps draining. A reliable event arriving on a full queue would
    // park the pump in `finish_batch_blocking` — with the guard already dropped
    // — and the frames below would be served for the wrong reason.
    let ui = {
        let events = running.events.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                while events.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };

    let flood = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut line = vec![b'x'; 4094];
            line.extend_from_slice(b"\r\n");
            while !stop.load(Ordering::Relaxed) {
                peer.output(&line);
            }
            peer
        })
    };

    // "The pump is mid-burst": it holds the engine. Nothing the pump publishes
    // can be the gate — a batch that never ends publishes nothing, which is the
    // starvation itself. `try_lock` does not raise the demand, so probing here
    // cannot be what makes the pump yield below.
    assert!(
        wait_until(Duration::from_secs(10), || running
            .term
            .try_lock()
            .is_none()),
        "the pump never took the engine lock"
    );

    // `lock_for_render()` raises a one-shot flag and *then* blocks, so a pump
    // asking inside that window consumes the only signal and the frame parks
    // invisibly — a race in `TerminalHandle`, not in this loop, measured and
    // recorded as the packet's gap 6. It made this test fail in the workspace
    // gate, where every other test loads the machine and widens the window.
    // This watchdog keeps a demand standing at the rate a 60 Hz renderer raises
    // one anyway, so the test measures the pump's hand-over latency rather than
    // that race, which has its own owner.
    let watchdog = {
        let term = Arc::clone(&running.term);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                term.raise_render_demand();
                std::thread::sleep(Duration::from_millis(16));
            }
        })
    };

    // The frames run on their own thread and report through a channel, so a
    // pump that never yields fails on the deadline instead of hanging the run.
    let (report_tx, report_rx) = mpsc::channel();
    let renderer = {
        let term = Arc::clone(&running.term);
        std::thread::spawn(move || {
            let mut worst = Duration::ZERO;
            for _ in 0..FRAMES {
                let started = Instant::now();
                let frame = term.lock_for_render();
                worst = worst.max(started.elapsed());
                drop(frame);
                // Let the pump take the engine back, so the next frame queues
                // behind a running batch instead of re-entering an idle mutex.
                std::thread::sleep(Duration::from_millis(5));
            }
            report_tx.send(worst)
        })
    };

    let deadline = HANDOVER_BOUND * FRAMES + Duration::from_secs(1);
    let report = report_rx.recv_timeout(deadline);

    stop.store(true, Ordering::Relaxed);
    let _peer = flood.join().unwrap();
    renderer.join().unwrap().expect("the frame thread reports");
    watchdog.join().unwrap();
    ui.join().unwrap();

    let worst = report.unwrap_or_else(|_| {
        panic!("no frame reached the engine within {deadline:?} of a flooding pump")
    });
    assert!(
        worst < HANDOVER_BOUND,
        "a frame waited {worst:?} behind the flooding pump (bound {HANDOVER_BOUND:?})"
    );
}

/// What the hand-over costs when a renderer really is taking frames: the pump
/// gives the engine up about sixty times a second instead of keeping it for the
/// whole burst. A measurement, not a gate — run it explicitly:
///
/// ```text
/// cargo test -p oneterm-local-shell --profile fast-dev -- --ignored --nocapture flood_throughput
/// ```
#[test]
#[ignore = "measurement; run it explicitly"]
fn flood_throughput_while_a_renderer_takes_frames() {
    const WINDOW: Duration = Duration::from_secs(2);
    const LINE: usize = 4096;

    let (running, mut peer) = start_loop();
    let stop = Arc::new(AtomicBool::new(false));
    let _stop_flood = StopFlood(Arc::clone(&stop));

    let ui = {
        let events = running.events.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                while events.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    };
    let flood = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut line = vec![b'x'; LINE - 2];
            line.extend_from_slice(b"\r\n");
            while !stop.load(Ordering::Relaxed) {
                peer.output(&line);
            }
            peer
        })
    };
    let renderer = {
        let term = Arc::clone(&running.term);
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut frames = 0u32;
            while !stop.load(Ordering::Relaxed) {
                drop(term.lock_for_render());
                frames += 1;
                std::thread::sleep(Duration::from_millis(16));
            }
            frames
        })
    };

    let started = Instant::now();
    std::thread::sleep(WINDOW);
    let lines = running.state.absolute_line_count();
    let elapsed = started.elapsed();

    stop.store(true, Ordering::Relaxed);
    let _peer = flood.join().unwrap();
    let frames = renderer.join().unwrap();
    ui.join().unwrap();

    let mib = (lines * LINE) as f64 / (1024.0 * 1024.0);
    println!(
        "flood: {mib:.1} MiB in {elapsed:?} = {:.1} MiB/s, renderer got {frames} frames",
        mib / elapsed.as_secs_f64()
    );
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
