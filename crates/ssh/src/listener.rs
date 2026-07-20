//! `SshListener` — `EventListener` for the SSH session.
//!
//! Forwards alacritty events → `SessionEvent` (via `async_channel`, non-blocking
//! `try_send`) AND updates the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` → `cmd_tx` (channel data going out to SSH).
//!
//! Similar to `local/src/listener.rs` but replaces `Notifier` with `cmd_tx`
//! (async_channel::Sender<Cmd>).

use std::sync::Arc;
use std::sync::atomic::Ordering;

#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::AtomicU64;

use async_channel::Sender;
#[cfg(any(test, feature = "terminal-diagnostics"))]
use async_channel::TrySendError;
use log::warn;

use alacritty_terminal::event::{Event, EventListener};

use oneterm_terminal::SessionEvent;
use oneterm_terminal::{
    ColorFormatter, Osc133Kind, OscPayload, PendingColorQuery, SharedColorQueries,
    TerminalSecurityPolicy, new_color_queries, parse_cwd_url, parse_osc,
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
    /// Set when close has been requested — the tokio task checks this flag
    /// to ensure close is always honored even if Cmd::Close is dropped.
    closing: Arc<std::sync::atomic::AtomicBool>,
    /// Diagnostic counters for bounded queue failures.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    queue_counters: Arc<SshQueueCounters>,
}

impl SshListener {
    pub fn new(event_tx: Sender<SessionEvent>, cmd_tx: Sender<Cmd>, state: SharedState) -> Self {
        Self {
            event_tx,
            cmd_tx,
            state,
            color_queries: new_color_queries(),
            security: TerminalSecurityPolicy::default(),
            closing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            queue_counters: Arc::new(SshQueueCounters::default()),
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
    pub fn pty_write(&self, bytes: &[u8]) {
        log::debug!("SshListener::pty_write: {} bytes", bytes.len());
        if let Err(e) = self.cmd_tx.try_send(Cmd::Write(bytes.to_vec())) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.record_command_failure(&e);
            warn!("SshListener::pty_write: try_send fail: {e}");
        }
    }

    /// Whether close has been requested. The tokio task checks this flag to
    /// ensure it exits even if `Cmd::Close` was dropped due to a full queue.
    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Relaxed)
    }

    /// Resize the SSH channel (via cmd_tx → tokio task → channel.window_change).
    pub fn pty_resize(&self, rows: u16, cols: u16) {
        if let Err(e) = self.cmd_tx.try_send(Cmd::Resize(rows, cols)) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.record_command_failure(&e);
            warn!("SshListener: pty_resize fail: {e}");
        }
    }

    /// Close the SSH channel. Close is lifecycle-critical and must never be
    /// silently dropped. Uses a closing flag + try_send to ensure the task
    /// exits even if the command is dropped due to a full queue.
    pub fn pty_close(&self) {
        // Close is lifecycle-critical and must never be silently dropped.
        // Set the closing flag (checked by the tokio task) AND send Cmd::Close.
        // If try_send fails (queue full), the flag ensures the task exits.
        self.closing.store(true, Ordering::Relaxed);
        if let Err(e) = self.cmd_tx.try_send(Cmd::Close) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.record_command_failure(&e);
            warn!("SshListener: pty_close queued via flag (try_send: {e})");
        }
    }

    /// Forward a `SessionEvent` (non-blocking). Drops it if the channel is
    /// full/closed — acceptable since `Output` is debounced.
    pub fn forward(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.try_send(ev) {
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.record_event_failure(&e);
            warn!("SshListener: drop event (channel full/closed): {e:?}");
        }
    }

    /// Forward a lifecycle `SessionEvent` (`Exited`/`Closed`) using
    /// `send_blocking` to ensure it is never silently dropped.
    pub fn forward_lifecycle(&self, ev: SessionEvent) {
        if let Err(e) = self.event_tx.send_blocking(ev) {
            // send_blocking only fails if the channel is closed.
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            self.queue_counters
                .event_closed
                .fetch_add(1, Ordering::Relaxed);
            warn!("SshListener: lifecycle event lost (channel closed): {e:?}");
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

        listener.pty_write(sentinel);

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
    fn phase0_bounded_ssh_queues_drop_new_items_and_count_failures() {
        let (listener, events, commands) = make_listener(1, 1);
        commands.try_send(Cmd::Close).unwrap();
        events
            .try_send(SessionEvent::Title("first".into()))
            .unwrap();

        listener.pty_write(b"dropped command");
        listener.forward(SessionEvent::Bell);

        let diagnostics = listener.queue_diagnostics();
        assert_eq!(diagnostics.command_full, 1);
        assert_eq!(diagnostics.event_full, 1);
        assert_eq!(commands.len(), 1);
        assert_eq!(events.len(), 1);

        commands.close();
        events.close();
        listener.pty_resize(24, 80);
        listener.forward(SessionEvent::Bell);
        let diagnostics = listener.queue_diagnostics();
        assert_eq!(diagnostics.command_closed, 1);
        assert_eq!(diagnostics.event_closed, 1);
    }

    #[test]
    fn phase1_close_is_honored_even_when_command_queue_is_full() {
        let (listener, _events, commands) = make_listener(2, 1);
        // Fill the command queue to capacity.
        commands.try_send(Cmd::Write(b"x".to_vec())).unwrap();
        assert_eq!(commands.len(), 1);

        // A regular write would be dropped (queue full)...
        listener.pty_write(b"dropped");
        assert_eq!(commands.len(), 1);

        // ...but close sets the closing flag even if Cmd::Close is dropped.
        // The tokio task checks is_closing() to ensure it exits.
        listener.pty_close();
        assert!(listener.is_closing());
        // Cmd::Close was dropped (queue full), but the flag is set.
        assert_eq!(commands.len(), 1);

        // Now drain the queue and try again — Cmd::Close fits.
        assert!(matches!(commands.try_recv(), Ok(Cmd::Write(_))));
        listener.pty_close();
        assert!(matches!(commands.try_recv(), Ok(Cmd::Close)));
    }
}
