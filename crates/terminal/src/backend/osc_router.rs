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
use crate::osc::{AgentOsc, agent_support_reply, is_legacy_agent_notification, parse_agent_osc};
use crate::osc_agent::{AGENT_OSC, LEGACY_AGENT_OSC, LEGACY_AGENT_OSC_SUB, should_apply};
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
    /// produced and one the embedder produced, and that is a published contract:
    /// `docs/osc-agent-status.md` § 3.2 tells an agent to write
    /// `ESC ] 20308 ; 0 ST` followed by `ESC [ c` and to conclude "not
    /// supported" if DA1 comes back first. Hoisting DA1 out of the second pass
    /// made OneTerm fail its own detection idiom on the terminal that
    /// implements it. Byte order in, byte order out.
    pub fn drain(&self, batch: &EventBatch, out: &mut Vec<SessionEvent>) {
        for event in batch.iter() {
            self.handle(batch, event, out);
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
            // content before that batch's title, cwd or agent event. Forwarding
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
                // `OSC 9;7` is the agent channel's deprecated alias, and the
                // engine routes per number, so it arrives here looking like a
                // notification. The raw sequence arrives too; that is where it
                // is handled.
                if is_legacy_agent_notification(body) {
                    return;
                }
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
                        }
                        ShellMark::OutputEnd { exit_code } => st.last_exit_code = *exit_code,
                        _ => {}
                    }
                }
                out.push(SessionEvent::ShellIntegration(*mark));
            }
            // ── OSC 20308, and OSC 9;7 wearing OSC 9's clothes ─────────
            VtEvent::Osc {
                code,
                params,
                terminator,
                truncated,
            } => {
                let params: Vec<&[u8]> = batch.params(*params).collect();
                // Bookkeeping first, and *before* the truncation guard below:
                // § 3.1 says the alias is "parsed identically, counted, and
                // logged once per session", and an event that arrived on `9;7`
                // arrived on `9;7` whether or not the parser could hold all of
                // it. Counting it only when it survives would under-report
                // exactly the agent whose payloads are too big — the one most
                // worth telling the operator about.
                self.note_agent_osc(*code, &params);
                // A truncated payload is not a short payload, it is a corrupt
                // one: the base64 was cut mid-stream, so decoding it yields
                // either an error or — worse — a shorter valid event that the
                // agent never sent. Drop it before anything parses it, and
                // count it, so the loss is visible instead of arriving as
                // "malformed JSON" from a payload that was never malformed.
                //
                // Deliberately the agent channel only. Free-form text degrades
                // gracefully when it is cut (a truncated OSC 9 toast is still a
                // toast) and OSC 133's markers are far too short to truncate;
                // structured data does not degrade, it lies.
                if *truncated && is_agent_osc(*code, &params) {
                    let count = self.state.count_truncated_agent_osc();
                    log::debug!(
                        "OscRouter: agent status dropped, payload truncated by the \
                         parser's cap ({count} so far)"
                    );
                    return;
                }
                match parse_agent_osc(*code, &params) {
                    Some(AgentOsc::Status(ev)) => self.apply_agent_status(*ev, out),
                    // `OSC 20308;0` — the support query (spec §3.2). Answered
                    // here rather than in the engine because the engine only
                    // routes numbers: the protocol, its version and the
                    // terminal's name are the embedder's, exactly as the OSC 52
                    // and colour replies are. It still leaves under the engine
                    // guard, before the pump's yield check and before any UI
                    // event is flushed (R-37).
                    Some(AgentOsc::SupportQuery) => {
                        self.reply(agent_support_reply(*terminator).as_bytes())
                    }
                    None => log::debug!(
                        "OscRouter: forwarded OSC {code} is not the agent channel ({} params)",
                        params.len()
                    ),
                }
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

    /// The agent channel's bookkeeping, which `parse_osc` cannot do because it
    /// is a pure function and both facts are per-session
    /// (`docs/osc-agent-status.md` §3 and §3.1):
    ///
    /// * an `OSC 20308` sub-code the receiver does not implement is ignored and
    ///   **counted** — `2` and above are reserved, so this is how a future
    ///   extension aimed at an older OneTerm shows up instead of vanishing;
    /// * `OSC 9;7` is deprecated. It is counted, and logged **once**, not once
    ///   per event: a still-unported agent emits thousands, and the point is to
    ///   make it diagnosable, not to drown the log.
    fn note_agent_osc(&self, code: u32, params: &[&[u8]]) {
        let sub = params.get(1).copied();
        if code == AGENT_OSC && !matches!(sub, Some(b"0" | b"1")) {
            let count = self.state.count_unknown_agent_subcode();
            log::debug!(
                "OscRouter: OSC 20308 ignored, unrecognised sub-code {:?} ({count} so far)",
                sub.map(String::from_utf8_lossy)
            );
        } else if code == LEGACY_AGENT_OSC
            && sub == Some(LEGACY_AGENT_OSC_SUB)
            && self.state.count_legacy_agent_osc() == 1
        {
            log::debug!(
                "OscRouter: OSC 9;7 is deprecated and is dropped in the next release — \
                 the agent should emit OSC 20308;1 instead (docs/osc-agent-status.md §3.1)"
            );
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

    /// Apply one agent-status event, or drop it as a replay.
    ///
    /// `seq` dedup (spec §4.1 / §8.3): an event whose `seq` is at or below the
    /// last applied `seq` for the same agent id is a repeat, not news.
    fn apply_agent_status(
        &self,
        ev: crate::osc_agent::AgentStatusEvent,
        out: &mut Vec<SessionEvent>,
    ) {
        let apply = should_apply(&mut self.state.lock().last_agent_seq, &ev);
        if apply {
            log::debug!(
                "agent status applied & forwarded: agent={} type={} seq={}",
                ev.agent(),
                ev.type_name(),
                ev.seq()
            );
            out.push(SessionEvent::AgentStatus(Arc::new(ev)));
        } else {
            log::debug!(
                "agent status dropped by dedup: agent={} type={} seq={}",
                ev.agent(),
                ev.type_name(),
                ev.seq()
            );
        }
    }
}

/// Whether this OSC carries an agent-status payload, under either spelling.
///
/// The support query and the reserved sub-codes are excluded: they carry no
/// payload worth truncating, and a truncated `20308;<n>` should still be
/// counted as the unknown sub-code it is.
fn is_agent_osc(code: u32, params: &[&[u8]]) -> bool {
    let sub = params.get(1).copied();
    (code == AGENT_OSC && sub == Some(b"1"))
        || (code == LEGACY_AGENT_OSC && sub == Some(LEGACY_AGENT_OSC_SUB))
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
