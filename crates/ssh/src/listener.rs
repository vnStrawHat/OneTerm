//! `SshListener` — `EventListener` for the SSH session.
//!
//! Forwards alacritty events → `SessionEvent` through a bounded channel: repaint
//! hints are coalescible, while stateful events apply backpressure. Also updates
//! the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` → `cmd_tx` (channel data going out to SSH).
//!
//! Similar to `local/src/listener.rs` but replaces `Notifier` with `cmd_tx`
//! (async_channel::Sender<Cmd>).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::AtomicU64;

use async_channel::Sender;
use async_channel::TrySendError;
use log::warn;

use alacritty_terminal::event::{Event, EventListener};

use oneterm_terminal::{
    ColorFormatter, NotificationRateLimiter, Osc133Kind, OscPayload, PendingColorQuery,
    SharedColorQueries, TerminalSecurityPolicy, new_color_queries, parse_cwd_url, parse_osc,
};
use oneterm_terminal::{SessionEvent, TerminalError};

use crate::state::SharedState;

/// Maximum queued SSH command messages.
pub(crate) const SSH_COMMAND_QUEUE_CAPACITY: usize = 256;
/// Maximum aggregate payload bytes waiting for SSH transport delivery.
pub(crate) const SSH_COMMAND_BYTE_BUDGET: usize = 4 * 1024 * 1024;

/// Command sent from the main thread → tokio task (via async_channel).
#[derive(Debug)]
pub enum Cmd {
    /// Write bytes to the SSH channel (keystroke, paste, OSC response).
    Write(Vec<u8>),
    /// Apply the latest coalesced PTY size.
    Resize,
    /// Close the channel.
    Close,
}

#[derive(Default)]
struct PendingResize {
    latest: Option<(u16, u16)>,
    signal_enqueued: bool,
}

/// Snapshot of SSH command/event queue failures.
#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SshQueueDiagnostics {
    /// Command writes rejected because the command queue was full.
    pub command_full: u64,
    /// Command writes rejected because the command queue was closed.
    pub command_closed: u64,
    /// Session events rejected because the event queue was full.
    pub event_full: u64,
    /// Session events rejected because the event queue was closed.
    pub event_closed: u64,
    /// Aggregate write payload bytes currently queued or in flight.
    pub queued_write_bytes: usize,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Default)]
struct SshQueueCounters {
    command_full: AtomicU64,
    command_closed: AtomicU64,
    event_full: AtomicU64,
    event_closed: AtomicU64,
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
    /// Security policy for terminal-controlled data (title, clipboard, etc).
    /// SSH is remote: clipboard read/write default off.
    security: TerminalSecurityPolicy,
    notification_limiter: Arc<Mutex<NotificationRateLimiter>>,
    /// Set when close has been requested — the tokio task checks this flag
    /// to ensure close is always honored even if Cmd::Close is dropped.
    closing: Arc<std::sync::atomic::AtomicBool>,
    /// Diagnostic counters for bounded queue failures.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    queue_counters: Arc<SshQueueCounters>,
    /// Aggregate bytes reserved by queued or in-flight `Cmd::Write` messages.
    queued_write_bytes: Arc<AtomicUsize>,
    /// Latest resize and whether a queue wakeup marker is already pending.
    pending_resize: Arc<Mutex<PendingResize>>,
}

impl SshListener {
    pub fn new(event_tx: Sender<SessionEvent>, cmd_tx: Sender<Cmd>, state: SharedState) -> Self {
        Self {
            event_tx,
            cmd_tx,
            state,
            color_queries: new_color_queries(),
            security: TerminalSecurityPolicy::default(),
            notification_limiter: Arc::new(Mutex::new(NotificationRateLimiter::default())),
            closing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            queue_counters: Arc::new(SshQueueCounters::default()),
            queued_write_bytes: Arc::new(AtomicUsize::new(0)),
            pending_resize: Arc::new(Mutex::new(PendingResize::default())),
        }
    }

    /// Return bounded-queue failure counters.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub fn queue_diagnostics(&self) -> SshQueueDiagnostics {
        SshQueueDiagnostics {
            command_full: self.queue_counters.command_full.load(Ordering::Relaxed),
            command_closed: self.queue_counters.command_closed.load(Ordering::Relaxed),
            event_full: self.queue_counters.event_full.load(Ordering::Relaxed),
            event_closed: self.queue_counters.event_closed.load(Ordering::Relaxed),
            queued_write_bytes: self.queued_write_bytes.load(Ordering::Relaxed),
        }
    }

    #[cfg(any(test, feature = "terminal-diagnostics"))]
    fn record_command_failure<T>(&self, error: &TrySendError<T>) {
        let counter = match error {
            TrySendError::Full(_) => &self.queue_counters.command_full,
            TrySendError::Closed(_) => &self.queue_counters.command_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(any(test, feature = "terminal-diagnostics"))]
    fn record_event_failure<T>(&self, error: &TrySendError<T>) {
        let counter = match error {
            TrySendError::Full(_) => &self.queue_counters.event_full,
            TrySendError::Closed(_) => &self.queue_counters.event_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Write bytes to the SSH channel (via cmd_tx → tokio task → channel.data).
    pub fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        log::debug!("SshListener::pty_write: {} bytes", bytes.len());
        if self.is_closing() {
            return Err(TerminalError::Closed);
        }
        if !self.reserve_write_bytes(bytes.len()) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.queue_counters
                .command_full
                .fetch_add(1, Ordering::Relaxed);
            return Err(TerminalError::QueueFull);
        }

        match self.cmd_tx.try_send(Cmd::Write(bytes.to_vec())) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.release_write_bytes(bytes.len());
                #[cfg(any(test, feature = "terminal-diagnostics"))]
                self.record_command_failure(&error);
                match error {
                    TrySendError::Full(_) => Err(TerminalError::QueueFull),
                    TrySendError::Closed(_) => Err(TerminalError::Closed),
                }
            }
        }
    }

    fn reserve_write_bytes(&self, additional: usize) -> bool {
        self.queued_write_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(additional)
                    .filter(|&next| next <= SSH_COMMAND_BYTE_BUDGET)
            })
            .is_ok()
    }

    pub(crate) fn release_write_bytes(&self, bytes: usize) {
        self.queued_write_bytes.fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Whether close has been requested. The tokio task checks this flag to
    /// ensure it exits even if `Cmd::Close` was dropped due to a full queue.
    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Relaxed)
    }

    /// Resize the SSH channel (via cmd_tx → tokio task → channel.window_change).
    pub fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.cmd_tx
            .try_send(Cmd::Resize(rows, cols))
            .map_err(|error| {
                #[cfg(any(test, feature = "terminal-diagnostics"))]
                self.record_command_failure(&error);
                match error {
                    TrySendError::Full(_) => TerminalError::QueueFull,
                    TrySendError::Closed(_) => TerminalError::Closed,
                }
            })
    }

    /// Close the SSH channel. Close is lifecycle-critical and must never be
    /// silently dropped. Uses a closing flag + try_send to ensure the task
    /// exits even if the command is dropped due to a full queue.
    pub fn pty_close(&self) -> Result<(), TerminalError> {
        self.closing.store(true, Ordering::Relaxed);
        match self.cmd_tx.try_send(Cmd::Close) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
            Err(_error @ TrySendError::Closed(_)) => {
                #[cfg(any(test, feature = "terminal-diagnostics"))]
                self.record_command_failure(&_error);
                Err(TerminalError::Closed)
            }
        }
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
                            log::debug!("SshListener: coalesced repaint event");
                        }
                        TrySendError::Closed(_) => {
                            warn!("SshListener: event channel is closed");
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
                    warn!("SshListener: reliable event lost because channel is closed: {error:?}");
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
        st.title = self.security.sanitize_title(&title);
    }

    fn set_clipboard(&self, text: String) {
        // SSH is remote: clipboard writes default off.
        if let Some(validated) = self.security.validate_clipboard_write(&text, true) {
            self.state.lock().unwrap().clipboard = Some(validated.to_string());
        }
    }

    /// Handle an OSC forwarded by the engine (`Event::Osc`, OSC 7/9/133) — update
    /// the state cache and forward the matching `SessionEvent`.
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
                        log::debug!("SshListener: notification rate limit exceeded");
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

impl EventListener for SshListener {
    fn send_event(&self, event: Event) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            Event::Wakeup => self.forward(SessionEvent::Output),
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            Event::Title(t) => {
                let sanitized = self.security.sanitize_title(&t).unwrap_or_default();
                self.set_title(t);
                self.forward(SessionEvent::Title(sanitized));
            }
            Event::ResetTitle => {
                self.set_title(String::new());
                self.forward(SessionEvent::Title(String::new()));
            }
            // ── Clipboard (OSC 52 set) ─────────────────────────────
            Event::ClipboardStore(_, text) => {
                // SSH is remote: clipboard writes default off.
                if let Some(validated) = self.security.validate_clipboard_write(&text, true) {
                    let validated = validated.to_string();
                    self.set_clipboard(text);
                    self.forward(SessionEvent::Clipboard(Some(validated)));
                }
            }
            Event::ClipboardLoad(_, _) => {
                // SSH is remote: clipboard reads default off.
                if self.security.allow_clipboard_read(true) {
                    self.forward(SessionEvent::ClipboardRead);
                } else {
                    log::debug!("SSH: OSC 52 clipboard read refused (remote default off)");
                }
            }
            // ── Channel write (OSC/DA response) ─────────────────────────
            Event::PtyWrite(s) => {
                if let Err(error) = self.pty_write(s.as_bytes()) {
                    warn!("SshListener: PTY response delivery failed: {error}");
                }
            }
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
                let parsed = parse_osc(&refs);
                log::debug!(
                    "SshListener: Event::Osc recv with {} params, parsed = {}",
                    refs.len(),
                    parsed.is_some()
                );
                if let Some(payload) = parsed {
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

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, Once};

    use log::{LevelFilter, Log, Metadata, Record};
    use oneterm_terminal::SessionEvent;
    use oneterm_terminal::test_support::FakeTransport;

    use super::*;
    use crate::state::new_shared;

    struct CaptureLogger {
        records: Mutex<Vec<String>>,
    }

    impl Log for CaptureLogger {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }

        fn log(&self, record: &Record<'_>) {
            self.records.lock().unwrap().push(format!(
                "{} {} {}",
                record.level(),
                record.target(),
                record.args()
            ));
        }

        fn flush(&self) {}
    }

    static LOGGER: CaptureLogger = CaptureLogger {
        records: Mutex::new(Vec::new()),
    };
    static INSTALL_LOGGER: Once = Once::new();

    fn capture_logs() {
        INSTALL_LOGGER.call_once(|| {
            log::set_logger(&LOGGER).expect("test logger should install once");
            log::set_max_level(LevelFilter::Trace);
        });
        LOGGER.records.lock().unwrap().clear();
    }

    fn make_listener(
        event_capacity: usize,
        command_capacity: usize,
    ) -> (SshListener, FakeTransport<SessionEvent>, FakeTransport<Cmd>) {
        let events = FakeTransport::bounded(event_capacity);
        let commands = FakeTransport::bounded(command_capacity);
        let listener = SshListener::new(events.sender(), commands.sender(), new_shared());
        (listener, events, commands)
    }

    #[test]
    fn phase1_ssh_input_is_not_logged() {
        capture_logs();
        let (listener, _events, commands) = make_listener(4, 4);
        let sentinel = b"PHASE0_DO_NOT_LOG_SECRET_7fd65c";

        assert_eq!(listener.pty_write(sentinel), Ok(()));

        let records = LOGGER.records.lock().unwrap().clone();
        // No log record may contain the sentinel secret.
        assert!(
            records
                .iter()
                .all(|record| !record.contains("PHASE0_DO_NOT_LOG_SECRET_7fd65c")),
            "sentinel secret leaked into log records: {records:?}"
        );
        // The write itself must still be delivered.
        assert!(matches!(
            commands.try_recv(),
            Ok(Cmd::Write(bytes)) if bytes == sentinel
        ));
        // Byte count may be logged, but not content.
        assert!(
            records.iter().any(|record| record.contains("31 bytes")),
            "expected byte-count log, got: {records:?}"
        );
    }

    #[test]
    fn coalescible_ssh_repaint_events_are_counted_when_saturated() {
        let (listener, events, commands) = make_listener(1, 1);
        commands.try_send(Cmd::Close).unwrap();
        events.try_send(SessionEvent::Output).unwrap();

        assert_eq!(
            listener.pty_write(b"dropped command"),
            Err(TerminalError::QueueFull)
        );
        listener.forward(SessionEvent::Output);

        let diagnostics = listener.queue_diagnostics();
        assert_eq!(diagnostics.command_full, 1);
        assert_eq!(diagnostics.event_full, 1);
        assert_eq!(commands.len(), 1);
        assert_eq!(events.len(), 1);

        commands.close();
        events.close();
        assert_eq!(listener.pty_resize(24, 80), Err(TerminalError::Closed));
        listener.forward(SessionEvent::Output);
        let diagnostics = listener.queue_diagnostics();
        assert_eq!(diagnostics.command_closed, 1);
        assert_eq!(diagnostics.event_closed, 1);
    }

    #[test]
    fn reliable_ssh_events_wait_for_queue_capacity() {
        let (listener, events, _commands) = make_listener(1, 1);
        events.try_send(SessionEvent::Output).unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (finished_tx, finished_rx) = std::sync::mpsc::sync_channel(1);

        let sender = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            listener.forward(SessionEvent::Bell);
            finished_tx.send(()).unwrap();
        });

        started_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        assert!(
            finished_rx
                .recv_timeout(std::time::Duration::from_millis(20))
                .is_err()
        );
        assert_eq!(events.try_recv().unwrap(), SessionEvent::Output);
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        sender.join().unwrap();
        assert_eq!(events.try_recv().unwrap(), SessionEvent::Bell);
    }

    #[test]
    fn phase1_close_is_honored_even_when_command_queue_is_full() {
        let (listener, _events, commands) = make_listener(2, 1);
        // Fill the command queue to capacity.
        commands.try_send(Cmd::Write(b"x".to_vec())).unwrap();
        assert_eq!(commands.len(), 1);

        // A regular write would be dropped (queue full)...
        assert_eq!(
            listener.pty_write(b"dropped"),
            Err(TerminalError::QueueFull)
        );
        assert_eq!(commands.len(), 1);

        // ...but close sets the closing flag even if Cmd::Close is dropped.
        // The tokio task checks is_closing() to ensure it exits.
        assert_eq!(listener.pty_close(), Ok(()));
        assert!(listener.is_closing());
        // Cmd::Close was dropped (queue full), but the flag is set.
        assert_eq!(commands.len(), 1);

        // Now drain the queue and try again — Cmd::Close fits.
        assert!(matches!(commands.try_recv(), Ok(Cmd::Write(_))));
        assert_eq!(listener.pty_close(), Ok(()));
        assert!(matches!(commands.try_recv(), Ok(Cmd::Close)));
    }
}
