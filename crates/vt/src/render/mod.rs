//! Damage and the render-state hand-off: what the renderer gets, and when.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`,
//! contract: `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md`.
//!
//! Damage is the per-row [`crate::grid::SeqNo`] the grid already stamps, read
//! through a watermark each consumer owns. There is no reset pass and no
//! consumer can clear another's damage, which is what makes a second consumer
//! possible without an engine change.

mod demand;
mod modes;
mod palette;
mod row;
mod state;
mod sync;

pub use demand::Demand;
pub use modes::{ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting};
pub use palette::Palette;
pub use row::{RenderCell, RenderContent, RenderRow, StyleRun};
pub use state::{EngineView, RenderCursor, RenderState, RenderUpdate, Watermark};
pub use sync::{SYNC_REFRESH, SYNC_WATCHDOG, SyncState};

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_bench.rs"]
mod bench;
