//! Custom event loop — replacement for `alacritty_terminal::event_loop::EventLoop`.
//!
//! Unlike the alacritty EventLoop, this feeds PTY bytes to **both**
//! `ansi::Processor` (Term) AND `vte::Parser` (OscSink) in parallel, capturing
//! OSC 7 (cwd) and OSC 133 (shell integration markers) that alacritty drops.
//!
//! Reference: `alacritty_terminal::event_loop::EventLoop`.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::mpsc;

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite};
use alacritty_terminal::vte::Parser as VteParser;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use log::error;
use polling::{Event as PollEvent, Events, PollMode, Poller};

use oneterm_core::SessionEvent;
use oneterm_core::terminal::default_color_for_index;
use oneterm_core::terminal::osc::{Osc133Kind, OscPayload, OscSink, parse_cwd_url};

use crate::listener::LocalListener;
use crate::state::SharedState;

/// PTY read buffer size (1 MiB — same as alacritty).
const READ_BUFFER_SIZE: usize = 0x10_0000;

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

/// Notifier for the UI to send messages to the event loop (replaces
/// `EventLoopSender`).
#[derive(Clone)]
pub struct ShellNotifier {
    sender: mpsc::Sender<ShellMsg>,
    poller: std::sync::Arc<Poller>,
}

impl ShellNotifier {
    pub fn send(&self, msg: ShellMsg) -> io::Result<()> {
        self.sender
            .send(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        self.poller.notify()
    }
}

/// Custom event loop — PTY I/O + byte routing (Term + OscSink).
pub struct ShellEventLoop {
    pty: tty::Pty,
    term: std::sync::Arc<FairMutex<Term<LocalListener>>>,
    listener: LocalListener,
    msg_rx: mpsc::Receiver<ShellMsg>,
    poll: std::sync::Arc<Poller>,
    state: SharedState,
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
        let (tx, rx) = mpsc::channel();
        let notifier = ShellNotifier {
            sender: tx,
            poller: poll.clone(),
        };
        Ok((
            Self {
                pty,
                term,
                listener,
                msg_rx: rx,
                poll,
                state,
            },
            notifier,
        ))
    }

    /// Spawn the event loop thread. Returns the join handle.
    pub fn spawn(mut self) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("PTY reader".into())
            .spawn(move || {
                self.run();
            })
            .expect("spawn PTY reader thread")
    }

    fn run(&mut self) {
        let mut buf = [0u8; READ_BUFFER_SIZE];
        let mut processor = Processor::<StdSyncHandler>::new();
        let mut vte_parser = VteParser::new();
        let mut osc_sink = OscSink::default();
        let mut write_queue: VecDeque<Cow<'static, [u8]>> = VecDeque::new();

        // Register PTY with poller.
        let interest = PollEvent::readable(0);
        let poll_opts = PollMode::Level;
        if let Err(err) = unsafe { self.pty.register(&self.poll, interest, poll_opts) } {
            error!("ShellEventLoop: register error: {err}");
            return;
        }

        let mut events = Events::with_capacity(1024.try_into().unwrap());

        loop {
            events.clear();
            // Timeout: short poll to check channel messages.
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
                            self.listener.forward(SessionEvent::Exited(code));
                        }
                        self.term.lock().exit();
                        self.listener.send_event(Event::Wakeup);
                        let _ = self.pty.deregister(&self.poll);
                        return;
                    }
                    continue;
                }

                if event.readable {
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
                            None => terminal.insert(match self.term.try_lock_unfair() {
                                None if unprocessed >= READ_BUFFER_SIZE => self.term.lock_unfair(),
                                None => continue,
                                Some(t) => t,
                            }),
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

                        // Feed the SAME bytes to OscSink (via vte::Parser), in parallel.
                        vte_parser.advance(&mut osc_sink, &buf[..unprocessed]);

                        // Process OSC payloads (OSC 7, OSC 133, etc.).
                        while let Some(payload) = osc_sink.take() {
                            self.handle_osc(payload);
                        }

                        // Screen was just cleared (clear/cls/RIS) → bump clear_epoch
                        // so the UI resets per-line timestamps (gutter).
                        if osc_sink.take_clear() {
                            self.state.lock().unwrap().clear_epoch += 1;
                        }

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
                        for reply in replies {
                            self.listener.pty_write(reply.as_bytes());
                        }
                    }
                }
            }
        }
    }

    /// Handle OSC payload from OscSink — update state + forward events.
    fn handle_osc(&self, payload: OscPayload) {
        match payload {
            OscPayload::Cwd(url) => {
                let cwd = parse_cwd_url(&url);
                {
                    let mut st = self.state.lock().unwrap();
                    st.cwd = Some(cwd.clone());
                }
                self.listener.forward(SessionEvent::Cwd(cwd));
            }
            OscPayload::ShellIntegration(kind) => {
                {
                    let mut st = self.state.lock().unwrap();
                    match kind {
                        Osc133Kind::PromptStart => {
                            st.prompt_count = st.prompt_count.saturating_add(1);
                        }
                        Osc133Kind::OutputEnd { exit_code } => {
                            st.last_exit_code = exit_code;
                        }
                        _ => {}
                    }
                }
                self.listener.forward(SessionEvent::ShellIntegration(kind));
            }
            OscPayload::Clipboard { query, .. } => {
                // Set (query=false) is handled by alacritty's ClipboardStore.
                // Read (query=true) → ask the UI to reply with the clipboard.
                if query {
                    self.listener.forward(SessionEvent::ClipboardRead);
                }
            }
            OscPayload::Notification(msg) => {
                self.listener.forward(SessionEvent::Notification(msg));
            }
            OscPayload::Progress(progress) => {
                self.listener.forward(SessionEvent::Progress(progress));
            }
        }
    }
}
