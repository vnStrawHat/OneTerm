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
#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use async_channel::Sender;
#[cfg(any(test, feature = "terminal-diagnostics"))]
use async_channel::TrySendError;
use log::warn;

use oneterm_terminal::SessionEvent;
use oneterm_terminal::{
    ColorFormatter, Osc133Kind, OscPayload, PendingColorQuery, SharedColorQueries,
    TerminalSecurityPolicy, new_color_queries, parse_cwd_url, parse_osc,
};

use crate::event_loop::{ShellMsg, ShellNotifier};
use crate::state::SharedState;

/// Snapshot of local session-event queue failures.
#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalQueueDiagnostics {
    /// Session events rejected because the event queue was full.
    pub event_full: u64,
    /// Session events rejected because the event queue was closed.
    pub event_closed: u64,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Default)]
struct LocalQueueCounters {
    event_full: AtomicU64,
    event_closed: AtomicU64,
}

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
    /// Security policy for terminal-controlled data (title, clipboard, etc).
    security: TerminalSecurityPolicy,
    /// Diagnostic counters for bounded event-queue failures.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    queue_counters: Arc<LocalQueueCounters>,
}

impl LocalListener {
    pub fn new(event_tx: Sender<SessionEvent>, state: SharedState) -> Self {
        Self {
            event_tx,
            notifier: std::sync::Arc::new(Mutex::new(None)),
            state,
            color_queries: new_color_queries(),
            security: TerminalSecurityPolicy::default(),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            queue_counters: Arc::new(LocalQueueCounters::default()),
        }
    }

    /// Return bounded event-queue failure counters.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub fn queue_diagnostics(&self) -> LocalQueueDiagnostics {
        LocalQueueDiagnostics {
            event_full: self.queue_counters.event_full.load(Ordering::Relaxed),
            event_closed: self.queue_counters.event_closed.load(Ordering::Relaxed),
        }
    }

    #[cfg(any(test, feature = "terminal-diagnostics"))]
    fn record_event_failure<T>(&self, error: &TrySendError<T>) {
        let counter = match error {
            TrySendError::Full(_) => &self.queue_counters.event_full,
            TrySendError::Closed(_) => &self.queue_counters.event_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
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
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.record_event_failure(&e);
            warn!("LocalListener: drop event (channel full/closed): {e:?}");
        }
    }

    /// Forward a lifecycle `SessionEvent` (`Exited`/`Closed`) using
    /// `send_blocking` to ensure it is never silently dropped.
    pub fn forward_lifecycle(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.send_blocking(ev) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.queue_counters
                .event_closed
                .fetch_add(1, Ordering::Relaxed);
            warn!("LocalListener: lifecycle event lost (channel closed): {e:?}");
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
        // Sanitize: strip control chars, BiDi overrides, cap length.
        st.title = self.security.sanitize_title(&title);
    }

    fn set_clipboard(&self, text: String) {
        // Local session: is_remote = false.
        if let Some(validated) = self.security.validate_clipboard_write(&text, false) {
            self.state.lock().unwrap().clipboard = Some(validated.to_string());
        }
    }

    fn set_exit(&self, code: Option<i32>) {
        let mut st = self.state.lock().unwrap();
        st.alive = false;
        st.exit_code = code;
    }

    /// Handle an OSC forwarded by the engine (`Event::Osc`, OSC 7/9/133) — update
    /// the state cache and forward the matching `SessionEvent`. Called on the pump
    /// thread during `Processor::advance` (the `Term` lock is held, `state` is not).
    fn handle_osc_payload(&self, payload: OscPayload) {
        match payload {
            OscPayload::Cwd(url) => {
                let cwd = parse_cwd_url(&url);
                if let Some(sanitized) = self.security.sanitize_cwd(&cwd.to_string_lossy()) {
                    let path = std::path::PathBuf::from(&sanitized);
                    self.state.lock().unwrap().cwd = Some(path.clone());
                    self.forward(SessionEvent::Cwd(path));
                }
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
            OscPayload::Notification(msg) => {
                if let Some(sanitized) = self.security.sanitize_notification(&msg) {
                    self.forward(SessionEvent::Notification(sanitized));
                }
            }
            OscPayload::Progress(progress) => self.forward(SessionEvent::Progress(progress)),
            OscPayload::AgentStatus(ev) => {
                // OSC 9;7 seq dedup (spec §4.1 / §8.3): drop events whose
                // `seq` is <= the last applied `seq` for the same agent id.
                // `ev` is `Box<AgentStatusEvent>` (kept small on the parse
                // path); unbox into the `Arc` for the fan-out `SessionEvent`.
                let ev = *ev;
                let apply = oneterm_terminal::should_apply(
                    &mut self.state.lock().unwrap().last_agent_seq,
                    &ev,
                );
                if apply {
                    log::debug!(
                        "OSC 9;7 applied & forwarded: agent={} type={} seq={}",
                        ev.agent(),
                        ev.type_name(),
                        ev.seq()
                    );
                    self.forward(SessionEvent::AgentStatus(std::sync::Arc::new(ev)));
                } else {
                    log::debug!(
                        "OSC 9;7 dropped by dedup: agent={} type={} seq={}",
                        ev.agent(),
                        ev.type_name(),
                        ev.seq()
                    );
                }
            }
        }
    }
}

impl EventListener for LocalListener {
    fn send_event(&self, event: Event) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            Event::Wakeup => self.forward(SessionEvent::Output),
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            Event::Title(t) => {
                // Sanitize before storing and forwarding.
                let sanitized = self.security.sanitize_title(&t).unwrap_or_default();
                self.set_title(t);
                self.forward(SessionEvent::Title(sanitized));
            }
            Event::ResetTitle => {
                self.set_title(String::new());
                self.forward(SessionEvent::Title(String::new()));
            }
            // ── Clipboard (OSC 52 set) ─────────────────────────────────
            Event::ClipboardStore(_, text) => {
                // Validate before forwarding.
                if let Some(validated) = self.security.validate_clipboard_write(&text, false) {
                    let validated = validated.to_string();
                    self.set_clipboard(text);
                    self.forward(SessionEvent::Clipboard(Some(validated)));
                }
            }
            // OSC 52 load (query clipboard) — the program asked us to send the
            // clipboard back. Forward so the UI replies (see security note: this
            // exposes the local clipboard to programs, including remote via SSH).
            Event::ClipboardLoad(_, _) => {
                self.forward(SessionEvent::ClipboardRead);
            }
            // ── PTY write (OSC/DA response) ─────────────────────────────
            Event::PtyWrite(s) => self.pty_write(s.as_bytes()),
            // ── Process exit ────────────────────────────────────────────
            Event::ChildExit(status) => {
                let code = status.code();
                self.set_exit(code);
                self.forward_lifecycle(SessionEvent::Exited(code));
            }
            // ── Shutdown ────────────────────────────────────────────────
            Event::Exit => {
                // close() sends Msg::Shutdown directly; Exit here = info.
            }
            // ── Bell ──────────────────────────────────────────────────
            Event::Bell => {
                self.forward(SessionEvent::Bell);
            }
            // ── OSC 7/9/133 (fork: Handler::report_osc → Event::Osc) ────
            // Parse once from the single VT pass — no second vte::Parser.
            Event::Osc { params, .. } => {
                let refs: Vec<&[u8]> = params.iter().map(|p| p.as_slice()).collect();
                let parsed = parse_osc(&refs);
                log::debug!(
                    "LocalListener: Event::Osc recv with {} params, parsed = {}",
                    refs.len(),
                    parsed.is_some()
                );
                if let Some(payload) = parsed {
                    self.handle_osc_payload(payload);
                }
            }
            // ── Screen cleared (CSI 2J/3J, RIS) ─────────────────────────
            // Bump clear_epoch so the UI resets per-line gutter timestamps.
            Event::ClearScreen => {
                self.state.lock().unwrap().clear_epoch += 1;
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

    #[test]
    fn clear_screen_bumps_clear_epoch() {
        let (l, _rx) = listener();
        let before = l.state.lock().unwrap().clear_epoch;
        l.send_event(Event::ClearScreen);
        assert_eq!(l.state.lock().unwrap().clear_epoch, before + 1);
    }

    #[test]
    fn osc7_cwd_forwards_and_caches() {
        let (l, rx) = listener();
        l.send_event(Event::Osc {
            params: vec![b"7".to_vec(), b"file:///tmp".to_vec()],
            bell_terminated: true,
        });
        assert_eq!(
            rx.try_recv().unwrap(),
            SessionEvent::Cwd(std::path::PathBuf::from("/tmp"))
        );
        assert_eq!(
            l.state.lock().unwrap().cwd.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
    }

    #[test]
    fn osc133_prompt_forwards() {
        let (l, rx) = listener();
        l.send_event(Event::Osc {
            params: vec![b"133".to_vec(), b"A".to_vec()],
            bell_terminated: true,
        });
        assert!(matches!(
            rx.try_recv().unwrap(),
            SessionEvent::ShellIntegration(_)
        ));
    }

    #[test]
    fn osc97_agent_status_forwards() {
        // OSC 9;7;<base64-json> — the engine forwards it as
        // `Event::Osc { params: [b"9", b"7", <base64>] }`. The listener
        // must parse + dedup + forward a `SessionEvent::AgentStatus`.
        let (l, rx) = listener();
        let json = stringify!(
            {"v":1,"agent":"pi","type":"state",
             "seq":1,"ts":1700000000000,
             "state":"working","message":"hi"}
        );
        let params = oneterm_terminal::encode_osc97_params(json);
        l.send_event(Event::Osc {
            params,
            bell_terminated: true,
        });
        match rx.try_recv().unwrap() {
            SessionEvent::AgentStatus(ev) => {
                assert_eq!(ev.agent(), "pi");
                assert_eq!(ev.seq(), 1);
                assert_eq!(ev.type_name(), "state");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn osc97_dedup_drops_stale_seq() {
        // A second event with seq <= the last applied seq is dropped (spec §8.3).
        let (l, rx) = listener();
        let mk = |seq: u64| {
            let json = format!(
                "{{\"v\":1,\"agent\":\"pi\",\"type\":\"state\",
                 \"seq\":{seq},\"ts\":1700000000000,
                 \"state\":\"working\"}}"
            );
            oneterm_terminal::encode_osc97_params(&json)
        };
        l.send_event(Event::Osc {
            params: mk(5),
            bell_terminated: true,
        });
        assert!(matches!(
            rx.try_recv().unwrap(),
            SessionEvent::AgentStatus(_)
        ));
        // seq=5 again (<= last applied) — dropped, nothing forwarded.
        l.send_event(Event::Osc {
            params: mk(5),
            bell_terminated: true,
        });
        assert!(rx.try_recv().is_err());
        // seq=3 (< last applied) — also dropped.
        l.send_event(Event::Osc {
            params: mk(3),
            bell_terminated: true,
        });
        assert!(rx.try_recv().is_err());
        // seq=6 (> last applied) — forwarded.
        l.send_event(Event::Osc {
            params: mk(6),
            bell_terminated: true,
        });
        assert!(matches!(
            rx.try_recv().unwrap(),
            SessionEvent::AgentStatus(_)
        ));
    }

    #[test]
    fn clipboard_load_forwards_read_request() {
        let (l, rx) = listener();
        l.send_event(Event::ClipboardLoad(
            alacritty_terminal::term::ClipboardType::Clipboard,
            std::sync::Arc::new(|s: &str| s.to_string()),
        ));
        assert_eq!(rx.try_recv().unwrap(), SessionEvent::ClipboardRead);
    }
}

#[cfg(test)]
mod phase0_tests {
    use oneterm_terminal::SessionEvent;
    use oneterm_terminal::test_support::FakeTransport;

    use super::*;
    use crate::state::new_shared;

    #[test]
    fn phase0_bounded_local_event_queue_drops_new_items_and_counts_failures() {
        let events = FakeTransport::bounded(1);
        let listener = LocalListener::new(events.sender(), new_shared());
        events
            .try_send(SessionEvent::Title("first".into()))
            .unwrap();

        listener.forward(SessionEvent::Bell);

        assert_eq!(listener.queue_diagnostics().event_full, 1);
        assert_eq!(events.len(), 1);

        events.close();
        listener.forward(SessionEvent::Bell);
        assert_eq!(listener.queue_diagnostics().event_closed, 1);
    }
}
