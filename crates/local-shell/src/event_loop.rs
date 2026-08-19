//! Custom event loop — replacement for `alacritty_terminal::event_loop::EventLoop`.
//!
//! Feeds PTY bytes to the shared [`TerminalPump`] (`ansi::Processor` + OSC
//! routing + line accounting) in a **single pass**. OSC 7/9/133 and screen
//! clears (`CSI 2J/3J`, RIS) are surfaced by the OneTerm alacritty fork via
//! `Event::Osc` / `Event::ClearScreen` and handled by the shared `OscRouter` —
//! there is no second `vte::Parser`. See docs/terminal-fullscreen-perf/09-*.md.
//!
//! The loop is generic over the PTY (`EventedPty + OnResize`) so tests drive it
//! with an in-memory transport instead of a real shell (TEST-02).
//!
//! Reference: `alacritty_terminal::event_loop::EventLoop`.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, Options};
use log::error;
use polling::{Event as PollEvent, Events, PollMode, Poller};

use oneterm_core::{TerminalLogConfig, report_best_effort};
use oneterm_terminal::{TerminalPump, local_log_identity};

use crate::transport::{LocalListener, LocalTransport};

/// PTY read buffer size (1 MiB — same as alacritty). Heap-allocated: the owner
/// thread's default 2 MiB stack must not carry it (PERF-21).
const READ_BUFFER_SIZE: usize = 0x10_0000;
/// Poll events collected per `poll.wait` (PTY readable + child watcher).
const POLL_EVENT_CAPACITY: NonZeroUsize = match NonZeroUsize::new(1024) {
    Some(capacity) => capacity,
    None => unreachable!(),
};
/// Maximum queued local-shell command messages.
pub(crate) const LOCAL_COMMAND_QUEUE_CAPACITY: usize = 256;
/// Maximum aggregate input bytes queued or waiting for PTY delivery.
pub(crate) const LOCAL_COMMAND_BYTE_BUDGET: usize = 4 * 1024 * 1024;

/// Maximum parser lock samples retained in one two-second diagnostics window.
#[cfg(feature = "terminal-diagnostics")]
const LOCK_SAMPLE_CAPACITY: usize = 16_384;

#[cfg(feature = "terminal-diagnostics")]
fn record_lock_sample(samples: &mut Vec<u64>, started: std::time::Instant) {
    if samples.len() < LOCK_SAMPLE_CAPACITY {
        samples.push(started.elapsed().as_micros() as u64);
    }
}

/// Token used by `alacritty_terminal`'s PTY to signal child (signal) events.
///
/// `alacritty_terminal::tty::PTY_CHILD_EVENT_TOKEN` is `pub(crate)` on Unix (only
/// `pub` on Windows), so it is not accessible from this crate. Its value is fixed
/// at `1` in alacritty's `tty/unix.rs` and `tty/windows/mod.rs`; the read/write
/// token is `0`. We mirror that value here.
const PTY_CHILD_EVENT_TOKEN: usize = 1;

/// Request handed to [`ShellNotifier::send`].
///
/// Only `Input` travels through the bounded command queue; `Resize` is
/// coalesced into `ShellControl::pending_resize` and `Shutdown` sets the
/// `ShellControl::shutdown` flag, so the loop observes both out of band.
#[derive(Debug)]
pub(crate) enum ShellMsg {
    /// Data written to the PTY (keystroke, paste).
    Input(Cow<'static, [u8]>),
    /// Resize the PTY.
    Resize(WindowSize),
    /// Shut down the event loop.
    Shutdown,
}

#[derive(Default)]
struct ShellControl {
    pending_resize: std::sync::Mutex<Option<WindowSize>>,
    shutdown: AtomicBool,
    queued_input_bytes: AtomicUsize,
}

/// Notifier for the UI to send messages to the event loop (replaces
/// `EventLoopSender`).
#[derive(Clone)]
pub(crate) struct ShellNotifier {
    /// Bounded queue of PTY input payloads (the only queued request kind).
    sender: mpsc::SyncSender<Cow<'static, [u8]>>,
    poller: std::sync::Arc<Poller>,
    control: std::sync::Arc<ShellControl>,
}

impl ShellNotifier {
    /// Queue a message and wake the owner loop. Every accepted message calls
    /// `poller.notify()`, so the loop can wait without a timeout (CORR-18).
    pub(crate) fn send(&self, msg: ShellMsg) -> io::Result<()> {
        if !matches!(&msg, ShellMsg::Shutdown) && self.control.shutdown.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "local-shell command queue is closed",
            ));
        }
        match msg {
            ShellMsg::Input(bytes) => {
                let length = bytes.len();
                let reserved = self
                    .control
                    .queued_input_bytes
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                        current
                            .checked_add(length)
                            .filter(|&next| next <= LOCAL_COMMAND_BYTE_BUDGET)
                    })
                    .is_ok();
                if !reserved {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "local-shell command byte budget is full",
                    ));
                }
                if let Err(error) = self.sender.try_send(bytes) {
                    self.control
                        .queued_input_bytes
                        .fetch_sub(length, Ordering::AcqRel);
                    return Err(match error {
                        mpsc::TrySendError::Full(_) => io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "local-shell command queue is full",
                        ),
                        mpsc::TrySendError::Disconnected(_) => io::Error::new(
                            io::ErrorKind::BrokenPipe,
                            "local-shell command queue is closed",
                        ),
                    });
                }
            }
            ShellMsg::Resize(size) => {
                *self
                    .control
                    .pending_resize
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(size);
            }
            ShellMsg::Shutdown => {
                self.control.shutdown.store(true, Ordering::Release);
            }
        }
        self.poller.notify()
    }
}

/// Custom event loop — PTY I/O + byte routing (single `Term` parse pass; OSC/clear
/// surfaced via `Event::Osc` / `Event::ClearScreen`, no second parser).
pub(crate) struct ShellEventLoop<P: EventedPty + OnResize> {
    pty: P,
    term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
    pump: TerminalPump<LocalTransport>,
    input_rx: mpsc::Receiver<Cow<'static, [u8]>>,
    poll: std::sync::Arc<Poller>,
    control: std::sync::Arc<ShellControl>,
}

#[cfg(unix)]
fn pty_process_id(pty: &tty::Pty) -> io::Result<u32> {
    Ok(pty.child().id())
}

#[cfg(windows)]
fn pty_process_id(pty: &tty::Pty) -> io::Result<u32> {
    pty.child_watcher()
        .pid()
        .map(std::num::NonZeroU32::get)
        .ok_or_else(|| io::Error::other("ConPTY child process id is unavailable"))
}

impl ShellEventLoop<tty::Pty> {
    /// Spawn the PTY owner thread. The PTY is constructed, operated, and dropped there.
    pub(crate) fn spawn_owned(
        opts: Options,
        winsize: WindowSize,
        term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
        listener: LocalListener,
        program: PathBuf,
        logging: TerminalLogConfig,
    ) -> io::Result<(ShellNotifier, std::thread::JoinHandle<()>)> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("PTY owner".into())
            .spawn(move || {
                let result = tty::new(&opts, winsize, 0).and_then(|pty| {
                    let pid = pty_process_id(&pty)?;
                    listener
                        .logging()
                        .set_identity(local_log_identity(&program, pid));
                    if logging.enabled
                        && let Err(error) = listener.logging().start(&logging)
                    {
                        log::warn!("Local terminal automatic logging did not start: {error}");
                    }
                    Self::new(pty, term, listener.clone())
                });
                match result {
                    Ok((mut event_loop, notifier)) => {
                        listener.transport().set_notifier(notifier.clone());
                        // The spawner may have given up waiting; the loop still
                        // runs and exits on its own shutdown flag.
                        report_best_effort("PTY owner ready signal", ready_tx.send(Ok(notifier)));
                        event_loop.run();
                    }
                    Err(error) => {
                        report_best_effort(
                            "PTY owner spawn-failure signal",
                            ready_tx.send(Err(error.to_string())),
                        );
                    }
                }
            })?;
        match ready_rx.recv() {
            Ok(Ok(notifier)) => Ok((notifier, join)),
            Ok(Err(error)) => {
                // The owner thread has already returned; the join only collects
                // its (irrelevant) panic status.
                report_best_effort("join failed PTY owner", join.join().map_err(|_| "panicked"));
                Err(io::Error::other(error))
            }
            Err(error) => {
                report_best_effort("join failed PTY owner", join.join().map_err(|_| "panicked"));
                Err(io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))
            }
        }
    }
}

impl<P: EventedPty + OnResize> ShellEventLoop<P> {
    /// Create a new event loop around an already-open PTY. Call `run()` on the
    /// owner thread.
    pub(crate) fn new(
        pty: P,
        term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
        listener: LocalListener,
    ) -> io::Result<(Self, ShellNotifier)> {
        let poll = std::sync::Arc::new(Poller::new()?);
        let control = std::sync::Arc::new(ShellControl::default());
        let (tx, rx) = mpsc::sync_channel(LOCAL_COMMAND_QUEUE_CAPACITY);
        let notifier = ShellNotifier {
            sender: tx,
            poller: poll.clone(),
            control: control.clone(),
        };
        Ok((
            Self {
                pty,
                term,
                pump: TerminalPump::new(listener),
                input_rx: rx,
                poll,
                control,
            },
            notifier,
        ))
    }

    /// Run the loop until shutdown or child exit. Blocks the calling thread.
    pub(crate) fn run(&mut self) {
        let mut buf = vec![0u8; READ_BUFFER_SIZE].into_boxed_slice();
        let mut write_queue: VecDeque<Cow<'static, [u8]>> = VecDeque::new();

        // Register PTY with poller.
        let interest = PollEvent::readable(0);
        let poll_opts = PollMode::Level;
        if let Err(err) = unsafe { self.pty.register(&self.poll, interest, poll_opts) } {
            error!("ShellEventLoop: register error: {err}");
            return;
        }

        let mut events = Events::with_capacity(POLL_EVENT_CAPACITY);

        // Throughput and lock-hold instrumentation is intentionally absent from
        // normal builds. Enable `terminal-diagnostics` and DEBUG logging when
        // profiling a sustained-output session.
        #[cfg(feature = "terminal-diagnostics")]
        let diagnostics_enabled = log::log_enabled!(log::Level::Debug);
        #[cfg(feature = "terminal-diagnostics")]
        let mut stat_bytes: u64 = 0;
        #[cfg(feature = "terminal-diagnostics")]
        let mut stat_wait = std::time::Duration::ZERO;
        #[cfg(feature = "terminal-diagnostics")]
        let mut stat_parse = std::time::Duration::ZERO;
        #[cfg(feature = "terminal-diagnostics")]
        let mut stat_lock_hold_us: Vec<u64> = Vec::with_capacity(1024);
        #[cfg(feature = "terminal-diagnostics")]
        let mut stat_since = std::time::Instant::now();

        loop {
            events.clear();
            // No timeout: every command (`ShellNotifier::send`) and the child
            // watcher wake the poller, so an idle tab sleeps until something
            // happens (CORR-18 / PERF-22).
            #[cfg(feature = "terminal-diagnostics")]
            let wait_start = std::time::Instant::now();
            if let Err(err) = self.poll.wait(&mut events, None) {
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                error!("ShellEventLoop: poll error: {err}");
                break;
            }
            #[cfg(feature = "terminal-diagnostics")]
            if diagnostics_enabled {
                stat_wait += wait_start.elapsed();
            }

            if self.control.shutdown.load(Ordering::Acquire) {
                self.deregister_pty();
                return;
            }
            let pending_resize = self
                .control
                .pending_resize
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(size) = pending_resize {
                self.pty.on_resize(size);
            }

            // Drain queued input (non-blocking). Resize and shutdown never
            // travel through the queue — see `ShellMsg`.
            while let Ok(bytes) = self.input_rx.try_recv() {
                write_queue.push_back(bytes);
            }

            // Write pending data to PTY.
            while let Some(bytes) = write_queue.front() {
                match self.pty.writer().write(bytes) {
                    Ok(0) => break,
                    Ok(n) => {
                        self.control
                            .queued_input_bytes
                            .fetch_sub(n, Ordering::AcqRel);
                        if n >= bytes.len() {
                            write_queue.pop_front();
                        } else {
                            // Partial write — trim remaining.
                            let remaining = bytes[n..].to_vec();
                            write_queue.pop_front();
                            write_queue.push_front(Cow::Owned(remaining));
                            break;
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
                    Err(err) => {
                        error!("ShellEventLoop: write error: {err}");
                        break;
                    }
                }
            }

            // Process readable events.
            for event in events.iter() {
                if event.is_interrupt() {
                    continue;
                }

                if event.key == PTY_CHILD_EVENT_TOKEN {
                    if let Some(tty::ChildEvent::Exited(status)) = self.pty.next_child_event() {
                        self.term.lock().exit();
                        publish_child_exit(&self.pump, status);
                        self.deregister_pty();
                        return;
                    }
                    continue;
                }

                if event.readable {
                    #[cfg(feature = "terminal-diagnostics")]
                    let parse_start = diagnostics_enabled.then(std::time::Instant::now);
                    #[cfg(feature = "terminal-diagnostics")]
                    let mut lock_started = None;
                    let mut unprocessed = 0;
                    let mut processed = 0;
                    let mut terminal = None;

                    loop {
                        match self.pty.reader().read(&mut buf[unprocessed..]) {
                            Ok(0) if unprocessed == 0 => break,
                            Ok(got) => unprocessed += got,
                            Err(err) => match err.kind() {
                                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => {
                                    if unprocessed == 0 {
                                        break;
                                    }
                                }
                                _ => {
                                    error!("ShellEventLoop: read error: {err}");
                                    break;
                                }
                            },
                        }

                        // Lock terminal.
                        let terminal = match &mut terminal {
                            Some(t) => t,
                            None => {
                                let guard = match self.term.try_lock_unfair() {
                                    None if unprocessed >= READ_BUFFER_SIZE => {
                                        self.term.lock_unfair()
                                    }
                                    None => continue,
                                    Some(t) => t,
                                };
                                #[cfg(feature = "terminal-diagnostics")]
                                if diagnostics_enabled {
                                    lock_started = Some(std::time::Instant::now());
                                }
                                terminal.insert(guard)
                            }
                        };

                        // Feed bytes to Term (parse + absolute line accounting).
                        self.pump.advance(terminal, &buf[..unprocessed]);

                        processed += unprocessed;
                        unprocessed = 0;

                        // Do NOT break at MAX_LOCKED_READ — read until the pipe is empty.
                        // When the pipe is empty, try_read() stores a waker → the reader
                        // thread notifies when more data arrives → no stalling.
                        // The Term lock is held while reading, but FairMutex ensures the
                        // UI can acquire the lock once the event loop releases it.
                    }

                    // Answer OSC 10/11/12 color queries collected during parsing.
                    // Read the current color from `Term` (reusing the lock guard
                    // if still held), fall back to the theme default, then reply.
                    let queries = self.pump.take_color_queries();
                    if !queries.is_empty() {
                        let guard = terminal.take().unwrap_or_else(|| self.term.lock_unfair());
                        let replies = self.pump.color_replies(&guard, queries);
                        drop(guard);
                        #[cfg(feature = "terminal-diagnostics")]
                        if diagnostics_enabled {
                            if let Some(start) = lock_started.take() {
                                record_lock_sample(&mut stat_lock_hold_us, start);
                            }
                        }
                        self.pump.write_color_replies(replies);
                    }

                    drop(terminal);
                    #[cfg(feature = "terminal-diagnostics")]
                    if diagnostics_enabled {
                        if let Some(start) = lock_started.take() {
                            record_lock_sample(&mut stat_lock_hold_us, start);
                        }
                        if let Some(start) = parse_start {
                            stat_parse += start.elapsed();
                        }
                        stat_bytes += processed as u64;
                    }

                    // The `Term` lock is released: publish the line count,
                    // deliver reliable events (Bell/Title/OSC…) that did not
                    // fit in the queue during `advance`, waiting for the UI if
                    // needed, then post the batch's repaint hint so they are
                    // seen before it.
                    self.pump.finish_batch_blocking(processed > 0);
                }
            }

            #[cfg(feature = "terminal-diagnostics")]
            if diagnostics_enabled {
                // Keep the report DEBUG-only: idle sessions must not emit
                // periodic INFO records in production. The percentile samples
                // make sustained-output lock contention measurable without a
                // separate profiler build.
                let since = stat_since.elapsed();
                if since >= std::time::Duration::from_secs(2) {
                    let secs = since.as_secs_f64();
                    let mib_s = stat_bytes as f64 / (1024.0 * 1024.0) / secs;
                    let parse_ms = stat_parse.as_secs_f64() * 1000.0;
                    let wait_ms = stat_wait.as_secs_f64() * 1000.0;
                    let busy = stat_parse.as_secs_f64() / secs * 100.0;
                    stat_lock_hold_us.sort_unstable();
                    let percentile = |fraction: f64| {
                        stat_lock_hold_us
                            .get(
                                ((stat_lock_hold_us.len().saturating_sub(1)) as f64 * fraction)
                                    .ceil() as usize,
                            )
                            .copied()
                            .unwrap_or(0)
                    };
                    log::debug!(
                        "[PTY pump] {:.1} MiB/s parsed | parse={:.0}ms wait={:.0}ms over {:.1}s | pump {:.0}% busy ({}) | lock p95={}us p99={}us samples={}",
                        mib_s,
                        parse_ms,
                        wait_ms,
                        secs,
                        busy,
                        if busy > 50.0 {
                            "parse-bound: OneTerm parse/grid-update is the limiter"
                        } else if mib_s < 1.0 {
                            "idle"
                        } else {
                            "wait-bound: ConPTY/producer is the limiter"
                        },
                        percentile(0.95),
                        percentile(0.99),
                        stat_lock_hold_us.len(),
                    );
                    stat_bytes = 0;
                    stat_wait = std::time::Duration::ZERO;
                    stat_parse = std::time::Duration::ZERO;
                    stat_lock_hold_us.clear();
                    stat_since = std::time::Instant::now();
                }
            }
        }
    }

    /// Unregister the PTY from the poller on the way out. The PTY is dropped
    /// with `self` right after, so a failure only means a stale registration
    /// that dies with the poller.
    fn deregister_pty(&mut self) {
        report_best_effort(
            "ShellEventLoop deregister PTY",
            self.pty.deregister(&self.poll),
        );
    }
}

/// Record the child's exit and tell the UI the session is over.
///
/// `status` is `None` when the platform watcher could not read an exit code
/// (Windows: `GetExitCodeProcess` failed or the watcher disconnected). The
/// session is dead either way, so `alive` is cleared and `Exited`/`Closed` are
/// forwarded unconditionally — otherwise the tab would show a live-but-dead PTY
/// forever. Must run without the `Term` lock held (the lifecycle forwards block
/// until the UI has room).
fn publish_child_exit(
    pump: &TerminalPump<LocalTransport>,
    status: Option<std::process::ExitStatus>,
) {
    let code = status.and_then(|status| status.code());
    pump.publish_exit_blocking(code);
    pump.finish_batch_blocking(true);
    pump.publish_closed_blocking();
}

#[cfg(test)]
#[path = "event_loop_tests.rs"]
mod event_loop_tests;
