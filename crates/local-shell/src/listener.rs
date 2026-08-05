//! `LocalListener` — `EventListener` for the local PTY.
//!
//! Forwards alacritty events → `SessionEvent` through a bounded channel: repaint
//! hints are coalescible, while stateful events apply backpressure. Also updates
//! the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` through the owner-thread notifier.
//!
//! Both `Term<U>` and `EventLoop` receive a **clone** of the same listener
//! (Arc-shared) — Term sends Title/PtyWrite/ClipboardStore while parsing, and
//! EventLoop sends Wakeup/ChildExit after reading. See `docs/terminal-backend.md` §5.

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::{AtomicU64, Ordering};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use async_channel::Sender;
use async_channel::TrySendError;
use log::warn;

use oneterm_terminal::{
    ClipboardOrigin, ColorFormatter, NotificationRateLimiter, Osc133Kind, OscPayload,
    PendingColorQuery, SharedColorQueries, TerminalSecurityPolicy, new_color_queries,
    parse_cwd_url, parse_osc,
};
use oneterm_terminal::{SessionEvent, TerminalError};

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
    /// Notifier (ShellNotifier) for PTY writes — initialized on the owner thread.
    notifier: Arc<Mutex<Option<ShellNotifier>>>,
    /// State cache (title/clipboard/alive) — shared with `LocalSession`.
    state: SharedState,
    /// Pending OSC 10/11/12 color queries — enqueued here, answered by the event
    /// loop after each parse batch (when the `Term` lock is free to read colors).
    color_queries: SharedColorQueries,
    /// Security policy for terminal-controlled data (title, clipboard, etc).
    security: TerminalSecurityPolicy,
    notification_limiter: Arc<Mutex<NotificationRateLimiter>>,
    /// Diagnostic counters for bounded event-queue failures.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    queue_counters: Arc<LocalQueueCounters>,
}

fn map_notifier_error(error: std::io::Error) -> TerminalError {
    match error.kind() {
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::NotConnected => TerminalError::Closed,
        std::io::ErrorKind::WouldBlock => TerminalError::QueueFull,
        _ => TerminalError::Transport(error.to_string()),
    }
}

impl LocalListener {
    pub fn new(event_tx: Sender<SessionEvent>, state: SharedState) -> Self {
        Self {
            event_tx,
            notifier: Arc::new(Mutex::new(None)),
            state,
            color_queries: new_color_queries(),
            security: TerminalSecurityPolicy::default(),
            notification_limiter: Arc::new(Mutex::new(NotificationRateLimiter::default())),
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

    /// Set the notifier once the owner thread has constructed the event loop. Can be called
    /// on any clone (Arc-shared).
    pub fn set_notifier(&self, sender: ShellNotifier) {
        *self.notifier.lock().unwrap() = Some(sender);
    }

    /// Write bytes to the PTY (via ShellMsg::Input).
    pub fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        let sender = self
            .notifier
            .lock()
            .unwrap()
            .clone()
            .ok_or(TerminalError::Closed)?;
        sender
            .send(ShellMsg::Input(Cow::Owned(bytes.to_vec())))
            .map_err(map_notifier_error)
    }

    /// Resize the PTY (via ShellMsg::Resize).
    pub fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        let sender = self
            .notifier
            .lock()
            .unwrap()
            .clone()
            .ok_or(TerminalError::Closed)?;
        let sz = WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 0,
            cell_height: 0,
        };
        sender
            .send(ShellMsg::Resize(sz))
            .map_err(map_notifier_error)
    }
    /// Shut down the EventLoop (via ShellMsg::Shutdown).
    pub fn pty_shutdown(&self) -> Result<(), TerminalError> {
        let sender = self
            .notifier
            .lock()
            .unwrap()
            .clone()
            .ok_or(TerminalError::Closed)?;
        sender.send(ShellMsg::Shutdown).map_err(map_notifier_error)
    }
    /// Forward a session event according to its delivery policy.
    ///
    /// Repaint hints may be coalesced when the bounded queue is full. All other
    /// events use blocking send so a slow consumer creates backpressure instead
    /// of silently losing clipboard, notification, progress, agent, or lifecycle
    /// state transitions.
    pub fn forward(&self, ev: SessionEvent) {
        match ev.delivery_policy() {
            oneterm_terminal::SessionEventDelivery::Coalescible => {
                if let Err(error) = self.event_tx.try_send(ev) {
                    #[cfg(any(test, feature = "terminal-diagnostics"))]
                    self.record_event_failure(&error);
                    match error {
                        TrySendError::Full(_) => {
                            log::debug!("LocalListener: coalesced repaint event");
                        }
                        TrySendError::Closed(_) => {
                            warn!("LocalListener: event channel is closed");
                        }
                    }
                }
            }
            oneterm_terminal::SessionEventDelivery::Reliable => {
                if let Err(error) = self.event_tx.send_blocking(ev) {
                    #[cfg(any(test, feature = "terminal-diagnostics"))]
                    self.queue_counters
                        .event_closed
                        .fetch_add(1, Ordering::Relaxed);
                    warn!(
                        "LocalListener: reliable event lost because channel is closed: {error:?}"
                    );
                }
            }
        }
    }

    /// Forward a lifecycle `SessionEvent` using the reliable delivery policy.
    pub fn forward_lifecycle(&self, ev: SessionEvent) {
        debug_assert_eq!(
            ev.delivery_policy(),
            oneterm_terminal::SessionEventDelivery::Reliable
        );
        self.forward(ev);
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
        // Local session.
        if let Some(validated) = self
            .security
            .validate_clipboard_write(&text, ClipboardOrigin::Local)
        {
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
                    if self
                        .notification_limiter
                        .lock()
                        .unwrap()
                        .allow(&self.security)
                    {
                        self.forward(SessionEvent::Notification(sanitized));
                    } else {
                        log::debug!("LocalListener: notification rate limit exceeded");
                    }
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
                    self.forward(SessionEvent::AgentStatus(Arc::new(ev)));
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
                if let Some(validated) = self
                    .security
                    .validate_clipboard_write(&text, ClipboardOrigin::Local)
                {
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
            Event::PtyWrite(s) => {
                if let Err(error) = self.pty_write(s.as_bytes()) {
                    warn!("LocalListener: PTY response delivery failed: {error}");
                }
            }
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
#[path = "listener_tests.rs"]
mod listener_tests;
