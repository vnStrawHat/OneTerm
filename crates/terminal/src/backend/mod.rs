//! Shared backend pump layer — everything a terminal backend needs between the
//! transport (PTY or SSH channel) and the UI, so `oneterm-local-shell` and
//! `oneterm-ssh` only implement [`PtyTransport`] and their own read loop.
//!
//! - [`SessionState`] / [`SharedState`] — title/cwd/clipboard/exit/OSC caches
//!   read by the `TerminalSession` accessors, written by the router.
//! - [`SessionEventSink`] — delivery policy for `SessionEvent`s: repaint hints
//!   coalesce, reliable events never block under the `Term` lock and are
//!   flushed by the pump after each parse batch.
//! - [`OscRouter`] — the alacritty `EventListener`: routes `Event`s into state
//!   updates + `SessionEvent`s and applies the security policy.
//! - [`ColorQueryReplier`] — OSC 10/11/12 query replies.
//! - [`LineAccounting`] — absolute-line counter decoupled from scrollback.
//! - [`TerminalPump`] — glues the above around `ansi::Processor` so a backend
//!   read loop only feeds bytes and calls `finish_batch`.
//!
//! See `docs/terminal-backend.md` §5.

mod color_reply;
mod event_sink;
mod line_accounting;
mod osc_router;
mod pump;
mod state;
mod transport;

pub use color_reply::ColorQueryReplier;
pub use event_sink::{EventQueueDiagnostics, SessionEventSink};
pub use line_accounting::LineAccounting;
pub use osc_router::OscRouter;
pub use pump::{GridSize, TerminalPump};
pub use state::{
    DefaultColors, SessionState, SharedSessionState, SharedState, SharedStateCwdSource,
};
pub use transport::PtyTransport;

#[cfg(test)]
mod backend_tests;
