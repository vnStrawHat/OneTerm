//! `OscRouter` — the event drain shared by every backend.
//!
//! Routes engine events into the state cache + `SessionEvent`s: title, OSC 52
//! clipboard, OSC 7/9/133 side-channel payloads, screen clears, colour queries,
//! bell, and terminal replies (`VtEvent::Reply` → transport). The security
//! policy is applied here for both backends, so local and SSH cannot drift
//! (SEC-08).
//!
//! Since `US-0081` the engine returns events as values instead of calling back,
//! so this is a plain function over a drained [`EventBatch`]
//! (`events-and-api.md` § "`feed` and drain") rather than an `EventListener`
//! installed inside the terminal. Two consequences, both deliberate:
//!
//! * **Replies leave first** (R-37). `VtEvent::Reply` carries the DA1 / DSR /
//!   DECRQM answers, and conhost blocks for up to a second at session start
//!   waiting for DA1 — which is exactly when a burst of output is arriving.
//! * The [`SessionEventSink`] deferred tier is **kept**. The backends' read
//!   loops still call `finish_batch*` after the lock is released, and
//!   `US-0083` / `US-0084` are where the drain moves out from under the lock;
//!   deleting the tier here would change delivery ordering in the packet whose
//!   acceptance is "zero behaviour diff".

use std::sync::{Arc, Mutex, PoisonError};

use log::warn;
use oneterm_vt::{ColorKey, EventBatch, StringTerm, VtEvent};

use crate::logging::TerminalLogController;
use crate::osc::{Osc133Kind, OscPayload, parse_cwd_url, parse_osc};
use crate::osc_agent::should_apply;
use crate::osc_color::{ColorFormatter, PendingColorQuery};
use crate::security_policy::{ClipboardOrigin, NotificationRateLimiter, TerminalSecurityPolicy};
use crate::session::SessionEvent;

use super::{PtyTransport, SessionEventSink, SharedState};

/// Engine-event router for one session (see module docs).
#[derive(Clone)]
pub struct OscRouter<T: PtyTransport> {
    transport: T,
    events: SessionEventSink,
    state: SharedState,
    color_queries: Arc<Mutex<Vec<PendingColorQuery>>>,
    security: TerminalSecurityPolicy,
    clipboard_origin: ClipboardOrigin,
    notification_limiter: Arc<Mutex<NotificationRateLimiter>>,
    logging: TerminalLogController,
}

impl<T: PtyTransport> OscRouter<T> {
    /// Build a router with the default security policy. `clipboard_origin`
    /// selects the OSC 52 policy branch (remote reads/writes default off).
    pub fn new(
        transport: T,
        events: SessionEventSink,
        state: SharedState,
        clipboard_origin: ClipboardOrigin,
    ) -> Self {
        Self::with_security(
            transport,
            events,
            state,
            clipboard_origin,
            TerminalSecurityPolicy::default(),
        )
    }

    /// Build a router with an explicit security policy.
    pub fn with_security(
        transport: T,
        events: SessionEventSink,
        state: SharedState,
        clipboard_origin: ClipboardOrigin,
        security: TerminalSecurityPolicy,
    ) -> Self {
        Self {
            transport,
            events,
            state,
            color_queries: Arc::default(),
            security,
            clipboard_origin,
            notification_limiter: Arc::new(Mutex::new(NotificationRateLimiter::default())),
            logging: TerminalLogController::default(),
        }
    }

    /// The backend transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// The UI event sink.
    pub fn events(&self) -> &SessionEventSink {
        &self.events
    }

    /// The shared state cache.
    pub fn state(&self) -> &SharedState {
        &self.state
    }

    /// Per-session printable-output logger.
    pub fn logging(&self) -> &TerminalLogController {
        &self.logging
    }

    /// Whether any colour query is waiting for an answer.
    pub fn has_color_queries(&self) -> bool {
        !self.lock_color_queries().is_empty()
    }

    /// Forward a session event (see [`SessionEventSink::forward`]).
    pub fn forward(&self, ev: SessionEvent) {
        self.events.forward(ev);
    }

    /// Enqueue an OSC colour query; the pump answers it after the batch.
    pub fn queue_color_query(&self, index: usize, format: ColorFormatter) {
        self.lock_color_queries()
            .push(PendingColorQuery { index, format });
    }

    /// Drain the pending colour queries.
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        std::mem::take(&mut *self.lock_color_queries())
    }

    fn lock_color_queries(&self) -> std::sync::MutexGuard<'_, Vec<PendingColorQuery>> {
        self.color_queries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Route one parse batch: replies first, then everything in byte order.
    pub fn drain(&self, batch: &EventBatch) {
        for event in batch.iter() {
            if let VtEvent::Reply(span) = event {
                self.reply(batch.bytes(*span));
            }
        }
        for event in batch.iter() {
            if !matches!(event, VtEvent::Reply(_)) {
                self.handle(batch, event);
            }
        }
    }

    /// Route one event. `batch` owns every payload the event points at.
    pub fn handle(&self, batch: &EventBatch, event: &VtEvent) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            VtEvent::Repaint => self.forward(SessionEvent::Output),
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            VtEvent::Title(span) => self.set_title(batch.str(*span)),
            VtEvent::TitleReset => self.set_title(""),
            // ── Clipboard (OSC 52) ─────────────────────────────────────
            VtEvent::ClipboardStore { text, .. } => {
                self.store_clipboard(batch.str(*text).to_owned())
            }
            VtEvent::ClipboardLoad { .. } => {
                if self.security.allow_clipboard_read(self.clipboard_origin) {
                    self.forward(SessionEvent::ClipboardRead);
                } else {
                    log::debug!("OscRouter: OSC 52 clipboard read refused by policy");
                }
            }
            // ── Terminal reply (DA / DSR / DECRQM / XTVERSION) ──────────
            VtEvent::Reply(span) => self.reply(batch.bytes(*span)),
            // ── Bell ──────────────────────────────────────────────────
            VtEvent::Bell => self.forward(SessionEvent::Bell),
            // ── OSC 7/9/133 (the engine's OSC registration table) ───────
            VtEvent::Osc { params, .. } => {
                let params: Vec<&[u8]> = batch.params(*params).collect();
                match parse_osc(&params) {
                    Some(payload) => self.handle_osc_payload(payload),
                    None => log::debug!(
                        "OscRouter: unparsed VtEvent::Osc with {} params",
                        params.len()
                    ),
                }
            }
            // ── Screen cleared (CSI 2J/3J, RIS) ─────────────────────────
            VtEvent::ScreenCleared => self.state.bump_clear_epoch(),
            // ── OSC 4/10/11/12 colour query (`?`): answered by the pump
            //    after the batch, when the engine colours can be read ────
            VtEvent::ColorQuery { key, terminator } => {
                self.queue_color_query(key.index(), color_formatter(*key, *terminator));
            }
            // ── Row bookkeeping: a `RowId`-keyed consumer's business, and
            //    nothing above the seam speaks `RowId` until `US-0085` ───
            VtEvent::RowsScrolled(_) | VtEvent::RowsTrimmed { .. } => {}
            // ── Graphics: `US-0080` owns the producer ───────────────────
            VtEvent::GraphicReleased(_) => {}
        }
    }

    fn reply(&self, bytes: &[u8]) {
        if let Err(error) = self.transport.pty_write(bytes) {
            warn!("OscRouter: terminal reply delivery failed: {error}");
        }
    }

    fn set_title(&self, title: &str) {
        let sanitized = self.security.sanitize_title(title);
        self.state.lock().title = sanitized.clone();
        self.forward(SessionEvent::Title(sanitized.unwrap_or_default()));
    }

    fn store_clipboard(&self, text: String) {
        let Some(validated) = self
            .security
            .validate_clipboard_write(&text, self.clipboard_origin)
        else {
            log::debug!("OscRouter: OSC 52 clipboard write refused by policy");
            return;
        };
        let validated = validated.to_string();
        self.state.lock().clipboard = Some(validated.clone());
        self.forward(SessionEvent::Clipboard(Some(validated)));
    }

    /// Handle an OSC forwarded by the engine (OSC 7/9/133) — update the state
    /// cache and forward the matching `SessionEvent`.
    fn handle_osc_payload(&self, payload: OscPayload) {
        match payload {
            OscPayload::Cwd(url) => {
                let cwd = parse_cwd_url(&url);
                if let Some(sanitized) = self.security.sanitize_cwd(&cwd.to_string_lossy()) {
                    let path = std::path::PathBuf::from(&sanitized);
                    self.state.lock().cwd = Some(path.clone());
                    self.forward(SessionEvent::Cwd(path));
                }
            }
            OscPayload::ShellIntegration(kind) => {
                {
                    let mut st = self.state.lock();
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
                let Some(sanitized) = self.security.sanitize_notification(&msg) else {
                    return;
                };
                let allowed = self
                    .notification_limiter
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .allow();
                if allowed {
                    self.forward(SessionEvent::Notification(sanitized));
                } else {
                    log::debug!("OscRouter: notification rate limit exceeded");
                }
            }
            OscPayload::Progress(progress) => self.forward(SessionEvent::Progress(progress)),
            OscPayload::AgentStatus(ev) => {
                // OSC 9;7 seq dedup (spec §4.1 / §8.3): drop events whose `seq`
                // is <= the last applied `seq` for the same agent id. `ev` is
                // boxed on the parse path; unbox into the `Arc` for fan-out.
                let ev = *ev;
                let apply = should_apply(&mut self.state.lock().last_agent_seq, &ev);
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

/// The reply the reference formatted inside the engine.
///
/// Byte-for-byte the fork's `dynamic_color_sequence`
/// (`vendor/alacritty_terminal/src/term/mod.rs:1692-1705`): the OSC prefix the
/// question used, each channel doubled to 16-bit precision, and the same
/// terminator the question carried. The engine reports the request and leaves
/// the formatting here because only the embedder owns the theme fallback.
fn color_formatter(key: ColorKey, terminator: StringTerm) -> ColorFormatter {
    let prefix = key.query_prefix();
    let terminator = match terminator {
        StringTerm::Bel => "\x07",
        StringTerm::St => "\x1b\\",
    };
    Arc::new(move |color| {
        format!(
            "\x1b]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
            prefix, color.r, color.g, color.b, terminator
        )
    })
}
