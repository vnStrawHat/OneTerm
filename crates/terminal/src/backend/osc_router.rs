//! `OscRouter` — the alacritty `EventListener` shared by every backend.
//!
//! Routes engine events into the state cache + `SessionEvent`s: title, OSC 52
//! clipboard, OSC 7/9/133 side-channel payloads (`Event::Osc` from the OneTerm
//! fork), screen clears, colour queries, bell, and PTY responses (`PtyWrite`
//! → transport). The security policy is applied here for both backends, so
//! local and SSH cannot drift (SEC-08).
//!
//! `Term<OscRouter<T>>` and the pump hold clones of the same router (Arc
//! fields). `send_event` runs during `Processor::advance` with the `Term` lock
//! held — it never blocks (see [`SessionEventSink`]).

use std::sync::{Arc, Mutex, PoisonError};

use alacritty_terminal::event::{Event, EventListener};
use log::warn;

use crate::osc::{Osc133Kind, OscPayload, parse_cwd_url, parse_osc};
use crate::osc_agent::should_apply;
use crate::osc_color::{ColorFormatter, PendingColorQuery};
use crate::security_policy::{ClipboardOrigin, NotificationRateLimiter, TerminalSecurityPolicy};
use crate::session::SessionEvent;

use super::{ColorQueryReplier, PtyTransport, SessionEventSink, SharedState};

/// Engine-event router for one session (see module docs).
#[derive(Clone)]
pub struct OscRouter<T: PtyTransport> {
    transport: T,
    events: SessionEventSink,
    state: SharedState,
    color_queries: ColorQueryReplier,
    security: TerminalSecurityPolicy,
    clipboard_origin: ClipboardOrigin,
    notification_limiter: Arc<Mutex<NotificationRateLimiter>>,
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
            color_queries: ColorQueryReplier::new(),
            security,
            clipboard_origin,
            notification_limiter: Arc::new(Mutex::new(NotificationRateLimiter::default())),
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

    /// The pending colour-query queue.
    pub fn color_queries(&self) -> &ColorQueryReplier {
        &self.color_queries
    }

    /// Forward a session event (see [`SessionEventSink::forward`]).
    pub fn forward(&self, ev: SessionEvent) {
        self.events.forward(ev);
    }

    /// Enqueue an OSC colour query; the pump answers it after the batch.
    pub fn queue_color_query(&self, index: usize, format: ColorFormatter) {
        self.color_queries.enqueue(index, format);
    }

    /// Drain the pending colour queries.
    pub fn take_color_queries(&self) -> Vec<PendingColorQuery> {
        self.color_queries.take()
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

    /// Handle an OSC forwarded by the engine (`Event::Osc`, OSC 7/9/133) —
    /// update the state cache and forward the matching `SessionEvent`.
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
                    .allow(&self.security);
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

impl<T: PtyTransport> EventListener for OscRouter<T> {
    fn send_event(&self, event: Event) {
        match event {
            // ── Render signal ──────────────────────────────────────────
            Event::Wakeup => self.forward(SessionEvent::Output),
            // ── Title (OSC 0/2) ─────────────────────────────────────────
            Event::Title(title) => self.set_title(&title),
            Event::ResetTitle => self.set_title(""),
            // ── Clipboard (OSC 52) ─────────────────────────────────────
            Event::ClipboardStore(_, text) => self.store_clipboard(text),
            Event::ClipboardLoad(_, _) => {
                if self.security.allow_clipboard_read(self.clipboard_origin) {
                    self.forward(SessionEvent::ClipboardRead);
                } else {
                    log::debug!("OscRouter: OSC 52 clipboard read refused by policy");
                }
            }
            // ── PTY write (OSC/DA response) ─────────────────────────────
            Event::PtyWrite(text) => {
                if let Err(error) = self.transport.pty_write(text.as_bytes()) {
                    warn!("OscRouter: PTY response delivery failed: {error}");
                }
            }
            // ── Process exit (only alacritty's own EventLoop emits this;
            //    OneTerm pumps publish exit themselves) ─────────────────
            Event::ChildExit(status) => {
                let code = status.code();
                self.state.record_exit(code);
                self.forward(SessionEvent::Exited(code));
            }
            // ── Shutdown: `close()` drives the transport directly ────────
            Event::Exit => {}
            // ── Bell ──────────────────────────────────────────────────
            Event::Bell => self.forward(SessionEvent::Bell),
            // ── OSC 7/9/133 (fork: Handler::report_osc → Event::Osc) ────
            Event::Osc { params, .. } => {
                let refs: Vec<&[u8]> = params.iter().map(|p| p.as_slice()).collect();
                match parse_osc(&refs) {
                    Some(payload) => self.handle_osc_payload(payload),
                    None => {
                        log::debug!("OscRouter: unparsed Event::Osc with {} params", refs.len())
                    }
                }
            }
            // ── Screen cleared (CSI 2J/3J, RIS) ─────────────────────────
            Event::ClearScreen => self.state.bump_clear_epoch(),
            // ── OSC 10/11/12 colour query (`?`): answered by the pump after
            //    the batch, when the `Term` colours can be read ───────────
            Event::ColorRequest(index, format) => self.queue_color_query(index, format),
            // ── Ignored ─────────────────────────────────────────────────
            Event::MouseCursorDirty
            | Event::CursorBlinkingChange
            | Event::TextAreaSizeRequest(_) => {}
        }
    }
}
