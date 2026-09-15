//! The engine's outward events: values in a caller-owned batch, never callbacks.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md>
//!
//! An event that names a row names it by [`RowId`](crate::grid::RowId), and a
//! row id keeps naming the same content for as long as that content is live, so
//! a consumer may key a cache by it.

// The module is `event` (the name the design and its test filters use) and its
// files live in `events/`, one concept each.

pub(crate) mod batch;
mod vt_event;

pub use batch::EventBatch;
pub use vt_event::{ClipboardKind, FeedStats, Progress, ShellMark, StringTerm, VtEvent};

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
