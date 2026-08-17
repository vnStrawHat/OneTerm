//! `SshListener` — `EventListener` for the SSH session.
//!
//! Forwards alacritty events → `SessionEvent` through a bounded channel: repaint
//! hints are coalescible, while stateful events are reliable — they are never
//! dropped, but they are also never sent blocking from a `Term` callback (the
//! `Term` lock is held there). Reliable events that do not fit are deferred and
//! flushed by the tokio task after the parse batch, outside the lock. Also
//! updates the `SessionState` cache (title/clipboard/alive).
//! Routes `PtyWrite` → `cmd_tx` (channel data going out to SSH).
//!
//! Similar to `local/src/listener.rs` but replaces `Notifier` with `cmd_tx`
//! (async_channel::Sender<Cmd>).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::AtomicU64;

use async_channel::{Sender, TrySendError};
use log::warn;

use alacritty_terminal::event::{Event, EventListener};

use oneterm_terminal::{
    ClipboardOrigin, ColorFormatter, NotificationRateLimiter, Osc133Kind, OscPayload,
    PendingColorQuery, SessionEvent, SharedColorQueries, TerminalError, TerminalSecurityPolicy,
    new_color_queries, parse_cwd_url, parse_osc,
};

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
    /// Reliable events that did not fit in the event queue. `forward` runs from
    /// `Term` callbacks with the `Term` lock held, so it must never block; the
    /// tokio task drains this queue with `flush_reliable` after each parse
    /// batch. FIFO order among reliable events is preserved.
    deferred_reliable: Arc<Mutex<VecDeque<SessionEvent>>>,
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
            deferred_reliable: Arc::new(Mutex::new(VecDeque::new())),
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

    /// Resize the SSH channel, coalescing bursts to the latest dimensions.
    pub fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        if self.is_closing() {
            return Err(TerminalError::Closed);
        }
        let mut pending = self.pending_resize.lock().unwrap();
        pending.latest = Some((rows, cols));
        if pending.signal_enqueued {
            return Ok(());
        }
        pending.signal_enqueued = true;
        match self.cmd_tx.try_send(Cmd::Resize) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                pending.signal_enqueued = false;
                Ok(())
            }
            Err(_error @ TrySendError::Closed(_)) => {
                pending.signal_enqueued = false;
                #[cfg(any(test, feature = "terminal-diagnostics"))]
                self.record_command_failure(&_error);
                Err(TerminalError::Closed)
            }
        }
    }

    pub(crate) fn take_pending_resize(&self) -> Option<(u16, u16)> {
        let mut pending = self.pending_resize.lock().unwrap();
        pending.signal_enqueued = false;
        pending.latest.take()
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
    /// events are reliable: they are enqueued when the queue has room and
    /// deferred otherwise, so a slow consumer never loses clipboard,
    /// notification, progress, agent, or lifecycle state transitions. This
    /// never blocks — it is called from `Term` callbacks while the `Term` lock
    /// is held, and the UI thread needs that lock to drain the queue (blocking
    /// here would deadlock the app). Deferred events are delivered by
    /// `flush_reliable`, which the tokio task calls after every parse batch.
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
                let mut deferred = self.deferred_reliable.lock().unwrap();
                // Keep FIFO order: once something is deferred, everything after
                // it queues behind it until the next flush.
                if !deferred.is_empty() {
                    deferred.push_back(ev);
                    return;
                }
                match self.event_tx.try_send(ev) {
                    Ok(()) => {}
                    Err(TrySendError::Full(ev)) => deferred.push_back(ev),
                    Err(error @ TrySendError::Closed(_)) => {
                        #[cfg(any(test, feature = "terminal-diagnostics"))]
                        self.record_event_failure(&error);
                        warn!(
                            "SshListener: reliable event lost because channel is closed: {error:?}"
                        );
                    }
                }
            }
        }
    }

    /// Whether reliable events are waiting for `flush_reliable`.
    #[cfg(test)]
    pub(crate) fn has_deferred_reliable(&self) -> bool {
        !self.deferred_reliable.lock().unwrap().is_empty()
    }

    fn pop_deferred_reliable(&self) -> Option<SessionEvent> {
        self.deferred_reliable.lock().unwrap().pop_front()
    }

    /// Deliver deferred reliable events, waiting for queue capacity. Must be
    /// called **without** the `Term` lock held (the tokio task calls it after
    /// each parse batch and before lifecycle events).
    pub(crate) async fn flush_reliable(&self) {
        while let Some(ev) = self.pop_deferred_reliable() {
            if let Err(error) = self.event_tx.send(ev).await {
                #[cfg(any(test, feature = "terminal-diagnostics"))]
                self.queue_counters
                    .event_closed
                    .fetch_add(1, Ordering::Relaxed);
                warn!("SshListener: reliable event lost because channel is closed: {error:?}");
            }
        }
    }

    /// Blocking variant of `flush_reliable` for callers outside the tokio
    /// runtime (tests). Must not be called while the `Term` lock is held.
    #[cfg(test)]
    pub(crate) fn flush_reliable_blocking(&self) {
        while let Some(ev) = self.pop_deferred_reliable() {
            if let Err(error) = self.event_tx.send_blocking(ev) {
                self.queue_counters
                    .event_closed
                    .fetch_add(1, Ordering::Relaxed);
                warn!("SshListener: reliable event lost because channel is closed: {error:?}");
            }
        }
    }

    /// Forward a lifecycle `SessionEvent` and flush every deferred reliable
    /// event so the transition reaches the UI in order. Call from the tokio
    /// task only, without the `Term` lock held.
    pub(crate) async fn forward_lifecycle(&self, ev: SessionEvent) {
        debug_assert_eq!(
            ev.delivery_policy(),
            oneterm_terminal::SessionEventDelivery::Reliable
        );
        self.forward(ev);
        self.flush_reliable().await;
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
        if let Some(validated) = self
            .security
            .validate_clipboard_write(&text, ClipboardOrigin::Remote)
        {
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
                if let Some(validated) = self
                    .security
                    .validate_clipboard_write(&text, ClipboardOrigin::Remote)
                {
                    let validated = validated.to_string();
                    self.set_clipboard(text);
                    self.forward(SessionEvent::Clipboard(Some(validated)));
                }
            }
            Event::ClipboardLoad(_, _) => {
                // SSH is remote: clipboard reads default off.
                if self.security.allow_clipboard_read(ClipboardOrigin::Remote) {
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

// Substantial delivery/secret-safety tests live in a sibling `listener_tests.rs`.
#[cfg(test)]
#[path = "listener_tests.rs"]
mod listener_tests;
