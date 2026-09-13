//! Shared backend pump layer — everything a terminal backend needs between the
//! transport (PTY or SSH channel) and the UI, so `oneterm-local-shell` and
//! `oneterm-ssh` only implement [`PtyTransport`] and their own read loop.
//!
//! - [`SessionState`] / [`SharedState`] — title/cwd/clipboard/exit/OSC caches
//!   read by the `TerminalSession` accessors, written by the router.
//! - [`SessionEventSink`] — delivery policy for `SessionEvent`s: repaint hints
//!   coalesce and are dropped when the queue is full, everything else applies
//!   backpressure.
//! - [`OscRouter`] — the event drain: routes the engine's `VtEvent`s into state
//!   updates and a vector of `SessionEvent`s, and applies the security policy.
//! - [`TerminalPump`] — glues the above around `Terminal::feed` so a backend
//!   read loop only feeds bytes and calls `finish_batch`.
//!
//! `LineAccounting` is **gone** (`US-0082`): the engine counts output lines
//! exactly (`Terminal::lines_produced`, R-05), so the heuristic over
//! `total_lines` and its per-chunk scan for `\n` under the lock had nothing left
//! to do.
//!
//! See `docs/terminal-backend.md` §5.

mod event_sink;
mod osc_router;
mod pump;
mod state;
mod transport;

pub use event_sink::{EventQueueDiagnostics, SessionEventSink};
pub use osc_router::OscRouter;
pub use pump::{GridSize, TerminalPump};
pub use state::{DefaultColors, SessionState, SharedSessionState, SharedState};
pub use transport::PtyTransport;

#[cfg(test)]
mod backend_tests;
