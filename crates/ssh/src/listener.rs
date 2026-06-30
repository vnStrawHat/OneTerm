//! `SshListener` — `EventListener` cho SSH session.
//!
//! Forward event alacritty → `SessionEvent` (qua `async_channel`, non-blocking
//! `try_send`) VÀ cập nhật `SessionState` cache (title/clipboard/alive).
//! Route `PtyWrite` → `cmd_tx` (channel data đi ra SSH).
//!
//! Tương tự `local/src/listener.rs` nhưng thay `Notifier` bằng `cmd_tx`
//! (async_channel::Sender<Cmd>).

use async_channel::Sender;
use log::warn;

use alacritty_terminal::event::{Event, EventListener};

use oneterm_core::SessionEvent;

use crate::state::SharedState;

/// Lệnh gửi từ main thread → tokio task (qua async_channel).
#[derive(Debug)]
pub enum Cmd {
    /// Ghi byte vào SSH channel (keystroke, paste, OSC response).
    Write(Vec<u8>),
    /// Resize PTY (window_change).
    Resize(u16, u16),
    /// Đóng channel.
    Close,
}

/// `EventListener` cho SSH session. Clone-thân thiện (Arc fields) để chia sẻ
/// giữa `Term` và tokio task.
#[derive(Clone)]
pub struct SshListener {
    /// Channel phát `SessionEvent` cho UI (sub qua `subscribe`).
    event_tx: Sender<SessionEvent>,
    /// Channel gửi `Cmd` tới tokio task (bridge sync→async).
    cmd_tx: Sender<Cmd>,
    /// Cache state — chia sẻ với `SshSession`.
    state: SharedState,
}

impl SshListener {
    pub fn new(event_tx: Sender<SessionEvent>, cmd_tx: Sender<Cmd>, state: SharedState) -> Self {
        Self {
            event_tx,
            cmd_tx,
            state,
        }
    }

    /// Ghi byte vào SSH channel (qua cmd_tx → tokio task → channel.data).
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

    /// Resize SSH channel (qua cmd_tx → tokio task → channel.window_change).
    pub fn pty_resize(&self, rows: u16, cols: u16) {
        if let Err(e) = self.cmd_tx.try_send(Cmd::Resize(rows, cols)) {
            warn!("SshListener: pty_resize fail: {e}");
        }
    }

    /// Đóng SSH channel.
    pub fn pty_close(&self) {
        if let Err(e) = self.cmd_tx.try_send(Cmd::Close) {
            warn!("SshListener: pty_close fail: {e}");
        }
    }

    /// Forward `SessionEvent` (non-blocking). Bỏ qua nếu channel đầy/closed.
    pub fn forward(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.try_send(ev) {
            warn!("SshListener: drop event (channel đầy/closed): {e:?}");
        }
    }

    fn set_title(&self, title: String) {
        let mut st = self.state.lock().unwrap();
        st.title = if title.is_empty() { None } else { Some(title) };
    }

    fn set_clipboard(&self, text: String) {
        self.state.lock().unwrap().clipboard = Some(text);
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
                warn!("SshListener: ClipboardLoad (OSC 52 read) chưa hỗ trợ");
            }
            // ── Ghi channel (OSC/DA response) ───────────────────────────
            Event::PtyWrite(s) => self.pty_write(s.as_bytes()),
            // ── Process exit — SSH dùng ChannelMsg::ExitStatus, không qua đây ──
            Event::ChildExit(_) => {}
            // ── Shutdown ────────────────────────────────────────────────
            Event::Exit => {}
            // ── Bell ──────────────────────────────────────────────────
            Event::Bell => {
                self.forward(SessionEvent::Bell);
            }
            // ── Bỏ qua ──────────────────────────────────────────────────
            Event::MouseCursorDirty
            | Event::CursorBlinkingChange
            | Event::ColorRequest(_, _)
            | Event::TextAreaSizeRequest(_) => {}
        }
    }
}
