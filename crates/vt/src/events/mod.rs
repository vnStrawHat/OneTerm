//! The engine's outward events: values in a caller-owned batch, never callbacks.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md`,
//! contract: `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md`
//! clause 3.
//!
//! The module is `event` (the name the design and its test filters use) and its
//! files live in `events/`, one concept each.

mod batch;
mod vt_event;

pub use batch::EventBatch;
pub use vt_event::{ClipboardKind, FeedStats, StringTerm, VtEvent};

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
