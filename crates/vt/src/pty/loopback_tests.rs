//! The transport contract, implemented over loopback sockets.
//!
//! This is the proof that [`EventedReadWrite`], [`EventedPty`] and [`OnResize`]
//! describe a shape a caller can implement, not just a description of the two
//! platform backends. OneTerm's own local-shell adapter drives its whole event
//! loop against exactly this fake instead of a real shell.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use polling::{Event, Events, PollMode, Poller};

use super::*;

/// Two connected loopback sockets.
fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("loopback address");
    let client = TcpStream::connect(address).expect("connect loopback");
    let (server, _) = listener.accept().expect("accept loopback");
    client.set_nodelay(true).expect("nodelay");
    server.set_nodelay(true).expect("nodelay");
    (client, server)
}

/// The transport half of the fake: what the caller polls and reads.
struct LoopbackPty {
    io: TcpStream,
    child_signal: TcpStream,
    child_events: mpsc::Receiver<ChildEvent>,
    resizes: Arc<Mutex<Vec<WindowSize>>>,
}

/// The "child" half: what the test drives.
struct LoopbackChild {
    io: TcpStream,
    child_signal: TcpStream,
    child_events: mpsc::Sender<ChildEvent>,
    resizes: Arc<Mutex<Vec<WindowSize>>>,
}

fn loopback_pty() -> (LoopbackPty, LoopbackChild) {
    let (io_pty, io_child) = socket_pair();
    let (signal_pty, signal_child) = socket_pair();
    io_pty.set_nonblocking(true).expect("nonblocking io");
    signal_pty
        .set_nonblocking(true)
        .expect("nonblocking signal");
    let (events_tx, events_rx) = mpsc::channel();
    let resizes = Arc::new(Mutex::new(Vec::new()));

    (
        LoopbackPty {
            io: io_pty,
            child_signal: signal_pty,
            child_events: events_rx,
            resizes: resizes.clone(),
        },
        LoopbackChild {
            io: io_child,
            child_signal: signal_child,
            child_events: events_tx,
            resizes,
        },
    )
}

impl LoopbackChild {
    fn write_output(&mut self, bytes: &[u8]) {
        self.io.write_all(bytes).expect("child output");
        self.io.flush().expect("flush child output");
    }

    fn read_input(&mut self, len: usize) -> Vec<u8> {
        let mut input = vec![0u8; len];
        self.io
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("read timeout");
        self.io.read_exact(&mut input).expect("child input");
        input
    }

    fn exit(&mut self, status: Option<std::process::ExitStatus>) {
        self.child_events
            .send(ChildEvent::Exited(status))
            .expect("queue the exit");
        self.child_signal.write_all(&[1]).expect("signal the exit");
        self.child_signal.flush().expect("flush the signal");
    }
}

impl EventedReadWrite for LoopbackPty {
    type Reader = TcpStream;
    type Writer = TcpStream;

    unsafe fn register(
        &mut self,
        poller: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        // SAFETY: both sockets are owned by `self` and deregistered before it
        // is dropped.
        unsafe {
            poller.add_with_mode(&self.io, interest, mode)?;
            poller.add_with_mode(
                &self.child_signal,
                Event::readable(PTY_CHILD_EVENT_TOKEN),
                PollMode::Level,
            )
        }
    }

    fn reregister(
        &mut self,
        poller: &Arc<Poller>,
        mut interest: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        interest.key = PTY_READ_WRITE_TOKEN;
        poller.modify_with_mode(&self.io, interest, mode)?;
        poller.modify_with_mode(
            &self.child_signal,
            Event::readable(PTY_CHILD_EVENT_TOKEN),
            PollMode::Level,
        )
    }

    fn deregister(&mut self, poller: &Arc<Poller>) -> io::Result<()> {
        poller.delete(&self.io)?;
        poller.delete(&self.child_signal)
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
    fn on_resize(&mut self, size: WindowSize) -> io::Result<()> {
        self.resizes.lock().expect("resize log").push(size);
        Ok(())
    }
}

fn exit_status(code: i32) -> std::process::ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code as u32)
    }
}

/// Wait until one of the tokens shows up, and report which.
fn wait_for_token(poller: &Poller, events: &mut Events) -> Option<usize> {
    for _ in 0..50 {
        events.clear();
        poller
            .wait(events, Some(Duration::from_millis(200)))
            .expect("poll");
        if let Some(key) = events.iter().map(|event| event.key).next() {
            return Some(key);
        }
    }
    None
}

#[test]
fn loopback_implements_the_evented_contract() {
    let poller = Arc::new(Poller::new().expect("poller"));
    let (mut pty, mut child) = loopback_pty();
    let mut events = Events::new();

    // SAFETY: `pty` outlives the registration; it is deregistered below.
    unsafe {
        pty.register(&poller, Event::readable(0), PollMode::Level)
            .expect("register");
    }

    // Output reaches the caller on the read/write token.
    child.write_output(b"hello");
    assert_eq!(
        wait_for_token(&poller, &mut events),
        Some(PTY_READ_WRITE_TOKEN)
    );
    let mut buffer = [0u8; 5];
    pty.reader().read_exact(&mut buffer).expect("read output");
    assert_eq!(&buffer, b"hello");

    // Input reaches the child.
    pty.writer().write_all(b"typed").expect("write input");
    assert_eq!(child.read_input(5), b"typed");

    // A resize is delivered as a value, not a signal.
    pty.on_resize(WindowSize {
        rows: 30,
        cols: 100,
        cell_width: 8,
        cell_height: 16,
    })
    .expect("resize");
    let applied = *child
        .resizes
        .lock()
        .expect("resize log")
        .last()
        .expect("a resize");
    assert_eq!((applied.rows, applied.cols), (30, 100));

    // Child exit arrives on its own token, and exactly once.
    child.exit(Some(exit_status(0)));
    assert_eq!(
        wait_for_token(&poller, &mut events),
        Some(PTY_CHILD_EVENT_TOKEN)
    );
    let event = pty.next_child_event().expect("a child event");
    let ChildEvent::Exited(Some(status)) = event else {
        panic!("expected an exit status, got {event:?}");
    };
    assert_eq!(status.code(), Some(0));
    assert!(pty.next_child_event().is_none());

    pty.deregister(&poller).expect("deregister");
}

/// `Exited(None)` is the shape the platform watchers use when they cannot read a
/// code; it still says the session is over.
#[test]
fn an_exit_without_a_code_is_still_an_exit() {
    let (mut pty, mut child) = loopback_pty();
    child.exit(None);
    assert_eq!(pty.next_child_event(), Some(ChildEvent::Exited(None)));
}
