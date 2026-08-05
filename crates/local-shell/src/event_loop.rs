//! Custom event loop — replacement for `alacritty_terminal::event_loop::EventLoop`.
//!
//! Feeds PTY bytes to `ansi::Processor` (Term) in a **single pass**. OSC 7/9/133
//! and screen clears (`CSI 2J/3J`, RIS) are surfaced by the OneTerm alacritty fork
//! via `Event::Osc` / `Event::ClearScreen` and handled in `LocalListener` — there
//! is no longer a second `vte::Parser`. See docs/terminal-fullscreen-perf/09-*.md.
//!
//! Reference: `alacritty_terminal::event_loop::EventLoop`.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite, Options};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use log::error;
use polling::{Event as PollEvent, Events, PollMode, Poller};

use oneterm_terminal::SessionEvent;
use oneterm_terminal::default_color_for_index;

use crate::listener::LocalListener;
use crate::state::SharedState;

/// PTY read buffer size (1 MiB — same as alacritty).
const READ_BUFFER_SIZE: usize = 0x10_0000;
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

/// Message sent to the event loop.
#[derive(Debug)]
pub enum ShellMsg {
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
pub struct ShellNotifier {
    sender: mpsc::SyncSender<ShellMsg>,
    poller: std::sync::Arc<Poller>,
    control: std::sync::Arc<ShellControl>,
}

impl ShellNotifier {
    pub fn send(&self, msg: ShellMsg) -> io::Result<()> {
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
                if let Err(error) = self.sender.try_send(ShellMsg::Input(bytes)) {
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
                *self.control.pending_resize.lock().unwrap() = Some(size);
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
pub struct ShellEventLoop {
    pty: tty::Pty,
    term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
    listener: LocalListener,
    msg_rx: mpsc::Receiver<ShellMsg>,
    poll: std::sync::Arc<Poller>,
    state: SharedState,
    control: std::sync::Arc<ShellControl>,
}

impl ShellEventLoop {
    /// Create a new event loop. Call `spawn()` to run the thread.
    pub fn new(
        pty: tty::Pty,
        term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
        listener: LocalListener,
        state: SharedState,
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
                listener,
                msg_rx: rx,
                poll,
                state,
                control,
            },
            notifier,
        ))
    }

    /// Spawn the PTY owner thread. The PTY is constructed, operated, and dropped there.
    pub fn spawn_owned(
        opts: Options,
        winsize: WindowSize,
        term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
        listener: LocalListener,
        state: SharedState,
    ) -> io::Result<(ShellNotifier, std::thread::JoinHandle<()>)> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("PTY owner".into())
            .spawn(move || {
                let result = tty::new(&opts, winsize, 0)
                    .and_then(|pty| Self::new(pty, term, listener.clone(), state));
                match result {
                    Ok((mut event_loop, notifier)) => {
                        listener.set_notifier(notifier.clone());
                        let _ = ready_tx.send(Ok(notifier));
                        event_loop.run();
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error.to_string()));
                    }
                }
            })?;
        match ready_rx.recv() {
            Ok(Ok(notifier)) => Ok((notifier, join)),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(io::Error::other(error))
            }
            Err(error) => {
                let _ = join.join();
                Err(io::Error::new(io::ErrorKind::BrokenPipe, error.to_string()))
            }
        }
    }

    fn run(&mut self) {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut processor = Processor::<StdSyncHandler>::new();
        let mut write_queue: VecDeque<Cow<'static, [u8]>> = VecDeque::new();

        // Register PTY with poller.
        let interest = PollEvent::readable(0);
        let poll_opts = PollMode::Level;
        if let Err(err) = unsafe { self.pty.register(&self.poll, interest, poll_opts) } {
            error!("ShellEventLoop: register error: {err}");
            return;
        }

        let mut events = Events::with_capacity(1024.try_into().unwrap());

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
            // Timeout: short poll to check channel messages.
            #[cfg(feature = "terminal-diagnostics")]
            let wait_start = std::time::Instant::now();
            if let Err(err) = self
                .poll
                .wait(&mut events, Some(std::time::Duration::from_millis(50)))
            {
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
                let _ = self.pty.deregister(&self.poll);
                return;
            }
            if let Some(size) = self.control.pending_resize.lock().unwrap().take() {
                self.pty.on_resize(size);
            }

            // Drain channel messages (non-blocking).
            let mut shutdown = false;
            while let Ok(msg) = self.msg_rx.try_recv() {
                match msg {
                    ShellMsg::Input(bytes) => {
                        write_queue.push_back(bytes);
                    }
                    ShellMsg::Resize(sz) => {
                        self.pty.on_resize(sz);
                    }
                    ShellMsg::Shutdown => {
                        shutdown = true;
                        break;
                    }
                }
            }
            if shutdown {
                let _ = self.pty.deregister(&self.poll);
                return;
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
                        if let Some(status) = status {
                            let code = status.code();
                            {
                                let mut st = self.state.lock().unwrap();
                                st.alive = false;
                                st.exit_code = code;
                            }
                            self.listener.forward_lifecycle(SessionEvent::Exited(code));
                        }
                        self.term.lock().exit();
                        self.listener.send_event(Event::Wakeup);
                        let _ = self.pty.deregister(&self.poll);
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

                    // Load absolute line count tracking state.
                    let (mut absolute, mut prev_total) = {
                        let st = self.state.lock().unwrap();
                        (st.absolute_line_count, st.prev_total_lines)
                    };

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

                        // Feed bytes to Term (via ansi::Processor).
                        processor.advance(&mut **terminal, &buf[..unprocessed]);
                        let total_after = terminal.total_lines();
                        let screen_lines = terminal.screen_lines();

                        // Track absolute line count (decoupled from scrollback).
                        if total_after > prev_total {
                            // Scrollback not yet full — total_lines grows.
                            absolute += total_after - prev_total;
                        } else if total_after == prev_total && total_after > screen_lines {
                            // Scrollback full — total_lines unchanged but there is new output.
                            // Count \n in the buffer = number of dropped lines.
                            let newline_count =
                                buf[..unprocessed].iter().filter(|&&b| b == b'\n').count();
                            absolute += newline_count;
                        } else if total_after < prev_total {
                            // Clear / alt-screen / resize — reset absolute.
                            absolute = total_after;
                        }
                        prev_total = total_after;

                        processed += unprocessed;
                        unprocessed = 0;

                        // Do NOT break at MAX_LOCKED_READ — read until the pipe is empty.
                        // When the pipe is empty, try_read() stores a waker → the reader
                        // thread notifies when more data arrives → no stalling.
                        // The Term lock is held while reading, but FairMutex ensures the
                        // UI can acquire the lock once the event loop releases it.
                    }

                    if processed > 0 {
                        // Persist absolute line count tracking state.
                        {
                            let mut st = self.state.lock().unwrap();
                            st.absolute_line_count = absolute;
                            st.prev_total_lines = prev_total;
                        }
                        self.listener.send_event(Event::Wakeup);
                    }

                    // Answer OSC 10/11/12 color queries collected during parsing.
                    // Read the current color from `Term` (reusing the lock guard
                    // if still held), fall back to the theme default, then reply.
                    let queries = self.listener.take_color_queries();
                    if !queries.is_empty() {
                        let (def_fg, def_bg, def_cursor, def_ansi) = {
                            let st = self.state.lock().unwrap();
                            (
                                st.default_foreground,
                                st.default_background,
                                st.default_cursor,
                                st.default_ansi,
                            )
                        };
                        let guard = terminal.take().unwrap_or_else(|| self.term.lock_unfair());
                        let mut replies: Vec<String> = Vec::new();
                        for q in queries {
                            let color = guard.colors()[q.index].or_else(|| {
                                default_color_for_index(
                                    q.index,
                                    def_fg,
                                    def_bg,
                                    def_cursor,
                                    def_ansi.as_ref(),
                                )
                            });
                            if let Some(color) = color {
                                replies.push((q.format)(color));
                            }
                        }
                        drop(guard);
                        #[cfg(feature = "terminal-diagnostics")]
                        if diagnostics_enabled {
                            if let Some(start) = lock_started.take() {
                                record_lock_sample(&mut stat_lock_hold_us, start);
                            }
                        }
                        for reply in replies {
                            if let Err(error) = self.listener.pty_write(reply.as_bytes()) {
                                log::warn!("ShellEventLoop: OSC reply delivery failed: {error}");
                            }
                        }
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
}

#[cfg(test)]
#[path = "event_loop_tests.rs"]
mod event_loop_tests;
