//! `SshListener` — `EventListener` for the SSH session.
//!
//! Forwards alacritty events → `SessionEvent` (via `async_channel`, non-blocking
//! `try_send`) AND updates the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` → `cmd_tx` (channel data going out to SSH).
//!
//! Similar to `local/src/listener.rs` but replaces `Notifier` with `cmd_tx`
//! (async_channel::Sender<Cmd>).

use async_channel::Sender;
use log::warn;

use alacritty_terminal::event::{Event, EventListener};

use oneterm_core::SessionEvent;
use oneterm_core::terminal::{
    ColorFormatter, Osc133Kind, OscPayload, PendingColorQuery, SharedColorQueries,
    new_color_queries, parse_cwd_url, parse_osc,
};

use crate::state::SharedState;

/// Command sent from the main thread → tokio task (via async_channel).
#[derive(Debug)]
pub enum Cmd {
    /// Write bytes to the SSH channel (keystroke, paste, OSC response).
    Write(Vec<u8>),
    /// Resize the PTY (window_change).
    Resize(u16, u16),
    /// Close the channel.
    Close,
}

/// `EventListener` for the SSH session. Clone-friendly (Arc fields) for sharing
/// between `Term` and the tokio task.
#[derive(Clone)]
pub struct SshListener {
    /// Channel emitting `SessionEvent` to the UI (subscribe via `subscribe`).
    event_tx: Sender<SessionEvent>,
    /// Channel sending `Cmd` to the tokio task (sync→async bridge).
    cmd_tx: Sender<Cmd>,
    /// State cache — shared with `SshSession`.
    state: SharedState,
    /// Pending OSC 10/11/12 color queries — enqueued here, answered by the tokio
    /// task after each parse batch (when the `Term` lock is free to read colors).
    color_queries: SharedColorQueries,
}

impl SshListener {
    pub fn new(event_tx: Sender<SessionEvent>, cmd_tx: Sender<Cmd>, state: SharedState) -> Self {
        Self {
            event_tx,
            cmd_tx,
            state,
            color_queries: new_color_queries(),
        }
    }

    /// Write bytes to the SSH channel (via cmd_tx → tokio task → channel.data).
    pub fn pty_write(&self, bytes: &[u8]) {
        log::debug!(
            "SshListener::pty_write: {} bytes: {:?}",
            bytes.len(),
            String::from_utf8_lossy(bytes)
        );
        if let Err(e) = self.cmd_tx.try_send(Cmd::Write(bytes.to_vec())) {
            warn!("SshListener::pty_write: try_send fail: {e}");
        }
    }

    /// Resize the SSH channel (via cmd_tx → tokio task → channel.window_change).
    pub fn pty_resize(&self, rows: u16, cols: u16) {
        if let Err(e) = self.cmd_tx.try_send(Cmd::Resize(rows, cols)) {
            warn!("SshListener: pty_resize fail: {e}");
        }
    }

    /// Close the SSH channel.
    pub fn pty_close(&self) {
        if let Err(e) = self.cmd_tx.try_send(Cmd::Close) {
            warn!("SshListener: pty_close fail: {e}");
        }
    }

    /// Forward a `SessionEvent` (non-blocking). Drops it if the channel is full/closed.
    pub fn forward(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.try_send(ev) {
            warn!("SshListener: drop event (channel full/closed): {e:?}");
        }
    }

    /// Enqueue an OSC 10/11/12 color query (from `Event::ColorRequest`). Answered
    /// by the tokio task after the current parse batch.
    pub fn queue_color_query(&self, index: usize, format: ColorFormatter) {
        self.color_queries
            .lock()
            .unwrap()
            .push(PendingColorQuery { index, format });
    }

    /// Drain all pending color queries (called by the tokio task).
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        std::mem::take(&mut *self.color_queries.lock().unwrap())
    }

    fn set_title(&self, title: String) {
        let mut st = self.state.lock().unwrap();
        st.title = if title.is_empty() { None } else { Some(title) };
    }

    fn set_clipboard(&self, text: String) {
        self.state.lock().unwrap().clipboard = Some(text);
    }

    /// Handle an OSC forwarded by the engine (`Event::Osc`, OSC 7/9/133) — update
    /// the state cache and forward the matching `SessionEvent`.
    fn handle_osc_payload(&self, payload: OscPayload) {
        match payload {
            OscPayload::Cwd(url) => {
                let cwd = parse_cwd_url(&url);
                self.state.lock().unwrap().cwd = Some(cwd.clone());
                self.forward(SessionEvent::Cwd(cwd));
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
                self.forward(SessionEvent::ShellIntegration(kind));
            }
            OscPayload::Notification(msg) => self.forward(SessionEvent::Notification(msg)),
            OscPayload::Progress(progress) => self.forward(SessionEvent::Progress(progress)),
        }
    }
}

impl EventListener for SshListener {
    fn send_event(&self, event: Event) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            Event::Wakeup => self.forward(SessionEvent::Output),
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            Event::Title(t) => {
                self.set_title(t.clone());
                self.forward(SessionEvent::Title(t));
            }
            Event::ResetTitle => {
                self.set_title(String::new());
                self.forward(SessionEvent::Title(String::new()));
            }
            // ── Clipboard (OSC 52 set) ─────────────────────────────────
            Event::ClipboardStore(_, text) => {
                self.set_clipboard(text.clone());
                self.forward(SessionEvent::Clipboard(Some(text)));
            }
            Event::ClipboardLoad(_, _) => {
                self.forward(SessionEvent::ClipboardRead);
            }
            // ── Channel write (OSC/DA response) ─────────────────────────
            Event::PtyWrite(s) => self.pty_write(s.as_bytes()),
            // ── Process exit — SSH uses ChannelMsg::ExitStatus, not this path ──
            Event::ChildExit(_) => {}
            // ── Shutdown ────────────────────────────────────────────────
            Event::Exit => {}
            // ── Bell ──────────────────────────────────────────────────
            Event::Bell => {
                self.forward(SessionEvent::Bell);
            }
            // ── OSC 7/9/133 (fork: Handler::report_osc → Event::Osc) ────
            Event::Osc { params, .. } => {
                let refs: Vec<&[u8]> = params.iter().map(|p| p.as_slice()).collect();
                if let Some(payload) = parse_osc(&refs) {
                    self.handle_osc_payload(payload);
                }
            }
            // ── Screen cleared (CSI 2J/3J, RIS) ─────────────────────────
            Event::ClearScreen => {
                self.state.lock().unwrap().clear_epoch += 1;
            }
            // ── OSC 10/11/12 color query (`?`) ─────────────────────────
            // Enqueue; the tokio task reads the current color from `Term` after
            // the parse batch and writes the reply back to the SSH channel.
            Event::ColorRequest(index, format) => {
                self.queue_color_query(index, format);
            }
            // ── Ignored ─────────────────────────────────────────────────
            Event::MouseCursorDirty
            | Event::CursorBlinkingChange
            | Event::TextAreaSizeRequest(_) => {}
        }
    }
}
