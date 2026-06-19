//! `LocalListener` — `EventListener` cho local PTY.
//!
//! Forward event alacritty → `SessionEvent` (qua `async_channel`, non-blocking
//! `try_send`) VÀ cập nhật `SessionState` cache (title/clipboard/alive).
//! Route `PtyWrite` → `Notifier` (EventLoopSender, set sau `EventLoop::new`).
//!
//! Cả `Term<U>` và `EventLoop` đều nhận **clone** của cùng listener (Arc-shared)
//! — Term gửi Title/PtyWrite/ClipboardStore khi parse, EventLoop gửi
//! Wakeup/ChildExit sau khi read. Tham chiếu `docs/terminal-backend.md` §5.

use std::borrow::Cow;
use std::sync::Mutex;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoopSender, Msg};
use async_channel::Sender;
use log::warn;

use myterm2_core::SessionEvent;

use crate::state::SharedState;

/// `EventListener` cho local shell. Clone-thân thiện (Arc fields) để chia sẻ
/// giữa `Term` và `EventLoop`.
#[derive(Clone)]
pub struct LocalListener {
    /// Channel phát `SessionEvent` cho UI (sub qua `subscribe`).
    event_tx: Sender<SessionEvent>,
    /// Notifier (EventLoopSender) để ghi PTY — set sau `EventLoop::new`.
    notifier: std::sync::Arc<Mutex<Option<EventLoopSender>>>,
    /// Cache state (title/clipboard/alive) — chia sẻ với `LocalSession`.
    state: SharedState,
}

impl LocalListener {
    pub fn new(event_tx: Sender<SessionEvent>, state: SharedState) -> Self {
        Self {
            event_tx,
            notifier: std::sync::Arc::new(Mutex::new(None)),
            state,
        }
    }

    /// Set notifier sau khi `event_loop.channel()` có sẵn. Gọi trên bất kỳ
    /// clone nào (Arc-shared).
    pub fn set_notifier(&self, sender: EventLoopSender) {
        *self.notifier.lock().unwrap() = Some(sender);
    }

    /// Ghi byte vào PTY (qua Msg::Input).
    pub fn pty_write(&self, bytes: &[u8]) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            if let Err(e) = sender.send(Msg::Input(Cow::Owned(bytes.to_vec()))) {
                warn!("LocalListener: pty_write Msg::Input fail: {e:?}");
            }
        }
    }

    /// Resize PTY (qua Msg::Resize). Grid reflow do caller (`LocalSession`).
    pub fn pty_resize(&self, rows: u16, cols: u16) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            let sz = WindowSize {
                num_lines: rows,
                num_cols: cols,
                cell_width: 0,
                cell_height: 0,
            };
            if let Err(e) = sender.send(Msg::Resize(sz)) {
                warn!("LocalListener: pty_resize Msg::Resize fail: {e:?}");
            }
        }
    }

    /// Shutdown EventLoop (qua Msg::Shutdown).
    pub fn pty_shutdown(&self) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            if let Err(e) = sender.send(Msg::Shutdown) {
                warn!("LocalListener: pty_shutdown fail: {e:?}");
            }
        }
    }

    /// Forward `SessionEvent` (non-blocking). Bỏ qua nếu channel đầy/closed —
    /// `Output` debounce nên chấp nhận được.
    fn forward(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.try_send(ev) {
            warn!("LocalListener: drop event (channel đầy/closed): {e:?}");
        }
    }

    fn set_title(&self, title: String) {
        let mut st = self.state.lock().unwrap();
        // Chuỗi rỗng = reset (ResetTitle) → None.
        st.title = if title.is_empty() { None } else { Some(title) };
    }

    fn set_clipboard(&self, text: String) {
        self.state.lock().unwrap().clipboard = Some(text);
    }

    fn set_exit(&self, code: Option<i32>) {
        let mut st = self.state.lock().unwrap();
        st.alive = false;
        st.exit_code = code;
    }
}

impl EventListener for LocalListener {
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
            // OSC 52 load (query clipboard) — cần callback clipboard từ UI,
            // chưa wire → bỏ qua (log).
            Event::ClipboardLoad(_, _) => {
                warn!("LocalListener: ClipboardLoad (OSC 52 read) chưa hỗ trợ");
            }
            // ── Ghi PTY (OSC/DA response) ───────────────────────────────
            Event::PtyWrite(s) => self.pty_write(s.as_bytes()),
            // ── Process exit ────────────────────────────────────────────
            Event::ChildExit(status) => {
                let code = status.code();
                self.set_exit(code);
                self.forward(SessionEvent::Exited(code));
            }
            // ── Shutdown ────────────────────────────────────────────────
            Event::Exit => {
                // close() gửi Msg::Shutdown trực tiếp; Exit ở đây = info.
            }
            // ── Bỏ qua (chưa cần) ──────────────────────────────────────
            Event::MouseCursorDirty
            | Event::CursorBlinkingChange
            | Event::Bell
            | Event::ColorRequest(_, _)
            | Event::TextAreaSizeRequest(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::new_shared;
    use async_channel::bounded;

    fn listener() -> (LocalListener, async_channel::Receiver<SessionEvent>) {
        let (tx, rx) = bounded::<SessionEvent>(16);
        (LocalListener::new(tx, new_shared()), rx)
    }

    #[test]
    fn forwards_title_and_wakeup() {
        let (l, rx) = listener();
        l.send_event(Event::Title("hello".into()));
        l.send_event(Event::Wakeup);
        assert_eq!(rx.try_recv().unwrap(), SessionEvent::Title("hello".into()));
        assert_eq!(rx.try_recv().unwrap(), SessionEvent::Output);
        assert_eq!(l.state.lock().unwrap().title.as_deref(), Some("hello"));
    }

    #[test]
    fn reset_title_clears_cache() {
        let (l, _rx) = listener();
        l.send_event(Event::Title("x".into()));
        l.send_event(Event::ResetTitle);
        assert_eq!(l.state.lock().unwrap().title, None);
    }

    #[test]
    fn clipboard_store_caches_and_forwards() {
        let (l, rx) = listener();
        l.send_event(Event::ClipboardStore(
            alacritty_terminal::term::ClipboardType::Clipboard,
            "secret".into(),
        ));
        assert_eq!(
            rx.try_recv().unwrap(),
            SessionEvent::Clipboard(Some("secret".into()))
        );
        assert_eq!(l.state.lock().unwrap().clipboard.as_deref(), Some("secret"));
    }

    #[test]
    fn child_exit_sets_alive_false_and_code() {
        let (l, rx) = listener();
        let status = std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .status()
            .unwrap();
        l.send_event(Event::ChildExit(status));
        match rx.try_recv().unwrap() {
            SessionEvent::Exited(code) => assert_eq!(code, Some(0)),
            other => panic!("unexpected {other:?}"),
        }
        let st = l.state.lock().unwrap();
        assert!(!st.alive);
        assert_eq!(st.exit_code, Some(0));
    }

    #[test]
    fn pty_write_without_notifier_logs_not_panics() {
        let (l, _rx) = listener();
        l.send_event(Event::PtyWrite("x".into()));
    }
}