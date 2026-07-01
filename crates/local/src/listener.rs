//! `LocalListener` — `EventListener` for the local PTY.
//!
//! Forwards alacritty events → `SessionEvent` (via `async_channel`, non-blocking
//! `try_send`) AND updates the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` → `Notifier` (EventLoopSender, set after `EventLoop::new`).
//!
//! Both `Term<U>` and `EventLoop` receive a **clone** of the same listener
//! (Arc-shared) — Term sends Title/PtyWrite/ClipboardStore while parsing, and
//! EventLoop sends Wakeup/ChildExit after reading. See `docs/terminal-backend.md` §5.

use std::borrow::Cow;
use std::sync::Mutex;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use async_channel::Sender;
use log::warn;

use oneterm_core::SessionEvent;
use oneterm_core::terminal::{
    ColorFormatter, PendingColorQuery, SharedColorQueries, new_color_queries,
};

use crate::event_loop::{ShellMsg, ShellNotifier};
use crate::state::SharedState;

/// `EventListener` for the local shell. Clone-friendly (Arc fields) for sharing
/// between `Term` and `EventLoop`.
#[derive(Clone)]
pub struct LocalListener {
    /// Channel emitting `SessionEvent` to the UI (subscribe via `subscribe`).
    event_tx: Sender<SessionEvent>,
    /// Notifier (ShellNotifier) for PTY writes — set after ShellEventLoop::new().
    notifier: std::sync::Arc<Mutex<Option<ShellNotifier>>>,
    /// State cache (title/clipboard/alive) — shared with `LocalSession`.
    state: SharedState,
    /// Pending OSC 10/11/12 color queries — enqueued here, answered by the event
    /// loop after each parse batch (when the `Term` lock is free to read colors).
    color_queries: SharedColorQueries,
}

impl LocalListener {
    pub fn new(event_tx: Sender<SessionEvent>, state: SharedState) -> Self {
        Self {
            event_tx,
            notifier: std::sync::Arc::new(Mutex::new(None)),
            state,
            color_queries: new_color_queries(),
        }
    }

    /// Set the notifier once `ShellEventLoop::new()` is available. Can be called
    /// on any clone (Arc-shared).
    pub fn set_notifier(&self, sender: ShellNotifier) {
        *self.notifier.lock().unwrap() = Some(sender);
    }

    /// Write bytes to the PTY (via ShellMsg::Input).
    pub fn pty_write(&self, bytes: &[u8]) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            if let Err(e) = sender.send(ShellMsg::Input(Cow::Owned(bytes.to_vec()))) {
                warn!("LocalListener: pty_write fail: {e}");
            }
        }
    }

    /// Resize the PTY (via ShellMsg::Resize).
    pub fn pty_resize(&self, rows: u16, cols: u16) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            let sz = WindowSize {
                num_lines: rows,
                num_cols: cols,
                cell_width: 0,
                cell_height: 0,
            };
            if let Err(e) = sender.send(ShellMsg::Resize(sz)) {
                warn!("LocalListener: pty_resize fail: {e}");
            }
        }
    }

    /// Shut down the EventLoop (via ShellMsg::Shutdown).
    pub fn pty_shutdown(&self) {
        if let Some(sender) = self.notifier.lock().unwrap().as_ref() {
            if let Err(e) = sender.send(ShellMsg::Shutdown) {
                warn!("LocalListener: pty_shutdown fail: {e}");
            }
        }
    }

    /// Forward a `SessionEvent` (non-blocking). Drops it if the channel is
    /// full/closed — acceptable since `Output` is debounced.
    pub fn forward(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.try_send(ev) {
            warn!("LocalListener: drop event (channel full/closed): {e:?}");
        }
    }

    /// Enqueue an OSC 10/11/12 color query (from `Event::ColorRequest`). Answered
    /// by the event loop after the current parse batch.
    pub fn queue_color_query(&self, index: usize, format: ColorFormatter) {
        self.color_queries
            .lock()
            .unwrap()
            .push(PendingColorQuery { index, format });
    }

    /// Drain all pending color queries (called by the event loop).
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        std::mem::take(&mut *self.color_queries.lock().unwrap())
    }

    fn set_title(&self, title: String) {
        let mut st = self.state.lock().unwrap();
        // Empty string = reset (ResetTitle) → None.
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
            // OSC 52 load (query clipboard) — needs a clipboard callback from the
            // UI, not wired yet → ignore (log).
            Event::ClipboardLoad(_, _) => {
                warn!("LocalListener: ClipboardLoad (OSC 52 read) not supported yet");
            }
            // ── PTY write (OSC/DA response) ─────────────────────────────
            Event::PtyWrite(s) => self.pty_write(s.as_bytes()),
            // ── Process exit ────────────────────────────────────────────
            Event::ChildExit(status) => {
                let code = status.code();
                self.set_exit(code);
                self.forward(SessionEvent::Exited(code));
            }
            // ── Shutdown ────────────────────────────────────────────────
            Event::Exit => {
                // close() sends Msg::Shutdown directly; Exit here = info.
            }
            // ── Bell ──────────────────────────────────────────────────
            Event::Bell => {
                self.forward(SessionEvent::Bell);
            }
            // ── OSC 10/11/12 color query (`?`) ─────────────────────────
            // Enqueue; the event loop reads the current color from `Term`
            // after the parse batch and writes the reply back to the PTY.
            Event::ColorRequest(index, format) => {
                self.queue_color_query(index, format);
            }
            // ── Ignored (not needed yet) ───────────────────────────────
            Event::MouseCursorDirty
            | Event::CursorBlinkingChange
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
