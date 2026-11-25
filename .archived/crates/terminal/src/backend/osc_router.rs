//! `OscRouter` — the event drain shared by every backend.
//!
//! Routes engine events into the state cache + `SessionEvent`s: title, OSC 52
//! clipboard, the typed OSC 7 / 9 / 133 events, screen clears, colour queries,
//! bell, and terminal replies (`VtEvent::Reply` → transport). The engine parses
//! all of those; what happens here is **policy** — whether a directory may be
//! trusted, a notification shown, a clipboard written, and how often — applied
//! for both backends so local and SSH cannot drift (SEC-08).
//!
//! The engine returns events as values instead of calling back, so this is a
//! plain function over a drained [`EventBatch`] (`events-and-api.md` § "`feed`
//! and drain") rather than an `EventListener` installed inside the terminal.
//!
//! `US-0082` splits the drain by **what may block**, not by what an event means:
//!
//! * [`OscRouter::drain`] runs where the caller already holds the engine lock
//!   and does only the things that must happen there and cannot wait — write
//!   every reply to the transport as the batch reaches it (R-37: conhost blocks
//!   for up to a second at session start waiting for DA1, which is exactly when
//!   a burst is arriving), queue colour queries so the pump can answer them off
//!   the live engine colours, and update the `SharedState` caches.
//! * Everything the UI sees is **appended to the pump's pending vector** and
//!   sent by [`super::TerminalPump::finish_batch_blocking`] once the guard is
//!   dropped. That is what let the deferred tier and its deadlock rule go.

use std::sync::{Arc, Mutex, PoisonError};

use log::warn;
use oneterm_vt::{ColorKey, EventBatch, ShellMark, StringTerm, VtEvent};

use crate::logging::TerminalLogController;
use crate::osc_color::{ColorFormatter, PendingColorQuery};
use crate::program_status::{self, Osc7501};
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

    /// Enqueue an OSC colour query; the pump answers it after the batch.
    pub fn queue_color_query(&self, key: ColorKey, format: ColorFormatter) {
        self.lock_color_queries()
            .push(PendingColorQuery { key, format });
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

    /// Route one parse batch in byte order, writing replies to the transport as
    /// they are reached and appending the UI-facing events to `out`.
    ///
    /// Called with the engine lock held, so nothing here waits on the UI. `out`
    /// is [`super::TerminalPump`]'s pending vector — the pump owns it and
    /// lends it per `advance` — and it is flushed after the guard is dropped.
    ///
    /// **One pass, and the order is the contract.** This used to be two passes,
    /// writing every [`VtEvent::Reply`] before routing anything else, which was
    /// R-37's "replies first" read literally. R-37 is about *latency* — a reply
    /// must not wait behind the UI — and one pass still delivers that, because
    /// everything a reply can now queue behind is a push onto `out`, which never
    /// blocks and never leaves this function. Nothing here can wait.
    ///
    /// What two passes broke is *relative* order between a reply the engine
    /// produced and one the embedder produced, and programs depend on it: a
    /// support probe followed by `ESC [ c` reads "not supported" when DA1 comes
    /// back first. Byte order in, byte order out.
    ///
    /// The OSC 7501 summary is computed once, after the batch, and only when
    /// the batch carried a 7501 report or a prompt start: a report storm costs
    /// one `SessionEvent::ProgramStatus` at most, and a batch without either
    /// does not take the state lock for it.
    pub fn drain(&self, batch: &EventBatch, out: &mut Vec<SessionEvent>) {
        let mut program_status_touched = false;
        for event in batch.iter() {
            program_status_touched |= matches!(
                event,
                VtEvent::Osc {
                    code: program_status::OSC,
                    ..
                } | VtEvent::ShellMark(ShellMark::PromptStart)
            );
            self.handle(batch, event, out);
        }
        if program_status_touched {
            self.publish_program_status(out);
        }
    }

    /// Push `SessionEvent::ProgramStatus` when the summary changed.
    fn publish_program_status(&self, out: &mut Vec<SessionEvent>) {
        let changed = {
            let mut st = self.state.lock();
            let summary = st.program_status.summary();
            (summary != st.last_program_status).then(|| {
                st.last_program_status = summary;
                summary
            })
        };
        if let Some(summary) = changed {
            out.push(SessionEvent::ProgramStatus(summary));
        }
    }

    /// Route one event. `batch` owns every payload the event points at.
    fn handle(&self, batch: &EventBatch, event: &VtEvent, out: &mut Vec<SessionEvent>) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            //
            // Dropped on purpose. The engine appends `Repaint` to every batch
            // that dispatched anything, but the repaint hint has exactly one
            // owner and always has: `TerminalPump::finish_batch*`, which posts
            // it **after** the deferred reliable events so a frame never shows
            // content before that batch's title or cwd event. Forwarding
            // it here too would double the hints and put the first one first.
            VtEvent::Repaint => {}
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            VtEvent::Title(span) => self.set_title(batch.str(*span), out),
            VtEvent::TitleReset => self.set_title("", out),
            // ── Clipboard (OSC 52) ─────────────────────────────────────
            VtEvent::ClipboardStore { text, .. } => {
                self.store_clipboard(batch.str(*text).to_owned(), out)
            }
            VtEvent::ClipboardLoad { .. } => {
                if self.security.allow_clipboard_read(self.clipboard_origin) {
                    out.push(SessionEvent::ClipboardRead);
                } else {
                    log::debug!("OscRouter: OSC 52 clipboard read refused by policy");
                }
            }
            // ── Terminal reply (DA / DSR / DECRQM / XTVERSION) ──────────
            VtEvent::Reply(span) => self.reply(batch.bytes(*span)),
            // ── Bell ──────────────────────────────────────────────────
            VtEvent::Bell => out.push(SessionEvent::Bell),
            // ── OSC 7: the working directory the shell reports ──────────
            //
            // Policy, not parsing: the engine handed over a host and a path it
            // has deliberately not resolved, and deciding whether to trust them
            // is this crate's.
            VtEvent::Cwd { path, .. } => {
                if let Some(sanitized) = self.security.sanitize_cwd(batch.str(*path)) {
                    let dir = std::path::PathBuf::from(&sanitized);
                    self.state.lock().cwd = Some(dir.clone());
                    out.push(SessionEvent::Cwd(dir));
                }
            }
            // ── OSC 9;4: taskbar progress ──────────────────────────────
            VtEvent::Progress(progress) => out.push(SessionEvent::Progress(*progress)),
            // ── OSC 9: desktop notification ────────────────────────────
            VtEvent::Notification { body, .. } => {
                let body = batch.str(*body);
                let Some(sanitized) = self.security.sanitize_notification(body) else {
                    return;
                };
                let allowed = self
                    .notification_limiter
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .allow();
                if allowed {
                    out.push(SessionEvent::Notification(sanitized));
                } else {
                    log::debug!("OscRouter: notification rate limit exceeded");
                }
            }
            // ── OSC 133: shell integration ─────────────────────────────
            VtEvent::ShellMark(mark) => {
                // The one place a mark can be observed without a renderer.
                // Which marks a shell actually emits is the per-shell question
                // `docs/terminal-backend.md` §6.1.2 answers, and `RUST_LOG=debug`
                // is how a walk checks it against a live tab (`US-0136`).
                log::debug!("OscRouter: shell mark {mark:?}");
                {
                    let mut st = self.state.lock();
                    match mark {
                        ShellMark::PromptStart => {
                            st.prompt_count = st.prompt_count.saturating_add(1);
                            // OSC 7501 lifetime: a new prompt ends whatever was
                            // working, blocked or idle.
                            st.program_status.prompt_started();
                        }
                        ShellMark::OutputEnd { exit_code } => st.last_exit_code = *exit_code,
                        _ => {}
                    }
                }
                out.push(SessionEvent::ShellIntegration(*mark));
            }
            // ── OSC 7501: program status (`crate::program_status`) ──────
            VtEvent::Osc {
                code: program_status::OSC,
                params,
                terminator,
                truncated,
            } => {
                // A cut report is corrupt, not short: drop it.
                if *truncated {
                    log::debug!("OscRouter: OSC 7501 dropped, truncated by the parser's cap");
                    return;
                }
                let params: Vec<&[u8]> = batch.params(*params).collect();
                match program_status::parse(&params) {
                    // Answered in this pass, so a program that sends the probe
                    // and then `CSI c` sees this reply before DA1's.
                    Some(Osc7501::Probe) => self.reply(&program_status::probe_reply(*terminator)),
                    Some(Osc7501::Report { id, state }) => {
                        self.state.lock().program_status.apply(id, state);
                    }
                    None => log::debug!("OscRouter: malformed OSC 7501 report dropped"),
                }
            }
            // ── Any other forwarded OSC: none is routed ──────────────────
            VtEvent::Osc { code, .. } => {
                log::debug!("OscRouter: forwarded OSC {code} has no handler");
            }
            // ── Screen cleared (CSI 2J/3J, RIS) ─────────────────────────
            VtEvent::ScreenCleared => self.state.bump_clear_epoch(),
            // ── OSC 4/10/11/12 colour query (`?`): answered by the pump
            //    after the batch, when the engine colours can be read ────
            VtEvent::ColorQuery { key, terminator } => {
                self.queue_color_query(*key, color_formatter(*key, *terminator));
            }
            // ── Row bookkeeping: a `RowId`-keyed consumer's business, and
            //    nothing above the seam speaks `RowId` until `US-0085` ───
            VtEvent::RowsScrolled(_) | VtEvent::RowsTrimmed { .. } => {}
            // ── Graphics: the view's store still evicts by LRU ──────────
            VtEvent::GraphicReleased(_) => {}
            // ── Nothing above the seam speaks these yet: OSC 1 (icon name),
            //    OSC 22 (pointer shape) and OSC 50 (the engine already
            //    applied the new cursor shape to its own state) ───────────
            _ => {}
        }
    }

    fn reply(&self, bytes: &[u8]) {
        if let Err(error) = self.transport.pty_write(bytes) {
            warn!("OscRouter: terminal reply delivery failed: {error}");
        }
    }

    fn set_title(&self, title: &str, out: &mut Vec<SessionEvent>) {
        let sanitized = self.security.sanitize_title(title);
        self.state.lock().title = sanitized.clone();
        out.push(SessionEvent::Title(sanitized.unwrap_or_default()));
    }

    fn store_clipboard(&self, text: String, out: &mut Vec<SessionEvent>) {
        let Some(validated) = self
            .security
            .validate_clipboard_write(&text, self.clipboard_origin)
        else {
            log::debug!("OscRouter: OSC 52 clipboard write refused by policy");
            return;
        };
        let validated = validated.to_string();
        self.state.lock().clipboard = Some(validated.clone());
        out.push(SessionEvent::Clipboard(Some(validated)));
    }
}

/// The reply the reference formatted inside the engine.
///
/// Byte-for-byte what the fork's `dynamic_color_sequence` emitted (the vendored
/// tree went with `US-0087`; the shape is pinned by this module's own tests):
/// the OSC prefix the question used, each channel doubled to 16-bit precision,
/// and the same
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
