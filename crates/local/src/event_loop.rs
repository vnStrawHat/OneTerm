//! Custom event loop — thay thế `alacritty_terminal::event_loop::EventLoop`.
//!
//! Khác alacritty EventLoop: feed byte PTY cho **cả** `ansi::Processor` (Term)
//! VÀ `vte::Parser` (OscSink) song song → bắt OSC 7 (cwd) + OSC 133 (shell
//! integration markers) mà alacritty drop.
//!
//! Tham chiếu: `alacritty_terminal::event_loop::EventLoop` (để tham khảo).

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::mpsc;

use alacritty_terminal::event::{Event, EventListener, OnResize, WindowSize};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty::{self, EventedPty, EventedReadWrite};
use alacritty_terminal::vte::Parser as VteParser;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use log::error;
use polling::{Event as PollEvent, Events, PollMode, Poller};

use myterm2_core::SessionEvent;
use myterm2_core::terminal::osc::{Osc133Kind, OscPayload, OscSink, parse_cwd_url};

use crate::listener::LocalListener;
use crate::state::SharedState;

/// Buffer size cho PTY read (1 MiB — same as alacritty).
const READ_BUFFER_SIZE: usize = 0x10_0000;

/// Message gửi tới event loop.
#[derive(Debug)]
pub enum ShellMsg {
    /// Data ghi vào PTY (keystroke, paste).
    Input(Cow<'static, [u8]>),
    /// Resize PTY.
    Resize(WindowSize),
    /// Shutdown event loop.
    Shutdown,
}

/// Notifier để UI gửi message tới event loop (thay `EventLoopSender`).
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
    /// Tạo event loop mới. Gọi `spawn()` để chạy thread.
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

    /// Spawn event loop thread. Trả join handle.
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
            // Timeout: short poll để check channel messages.
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

                if event.key == tty::PTY_CHILD_EVENT_TOKEN {
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

                        // Feed SAME bytes to OscSink (via vte::Parser) — song song.
                        vte_parser.advance(&mut osc_sink, &buf[..unprocessed]);

                        // Process OSC payloads (OSC 7, OSC 133, etc.).
                        while let Some(payload) = osc_sink.take() {
                            self.handle_osc(payload);
                        }

                        processed += unprocessed;
                        unprocessed = 0;

                        // KHÔNG break ở MAX_LOCKED_READ — đọc đến khi pipe empty.
                        // Khi pipe empty, try_read() stores waker → reader thread
                        // notify khi thêm data → không bị stuck.
                        // Term lock được giữ trong khi đọc, nhưng FairMutex đảm bảo
                        // UI acquire được lock sau khi event loop release.
                    }

                    if processed > 0 {
                        self.listener.send_event(Event::Wakeup);
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
            OscPayload::Clipboard { .. } => {
                // OSC 52 already handled by alacritty's EventListener.
            }
        }
    }
}
