//! Damage and the snapshot hand-off: what the renderer gets, and when.
//!
//! The embedder pulls: it asks for an update whenever it likes and gets back
//! only the rows whose content changed since its own last ask, keyed by row id.
//!
//! Damage is the per-row [`crate::grid::SeqNo`] the grid already stamps, read
//! through a watermark each consumer owns. There is no reset pass and no
//! consumer can clear another's damage, so two consumers can each keep their
//! own state over one terminal.
//!
//! Design notes:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md>

mod modes;
mod palette;
mod row;
mod state;
mod sync;

pub use modes::{ModeSnapshot, MouseEncoding, MouseProtocol, MouseReporting};
pub use palette::Palette;
pub use row::{SnapshotCell, SnapshotContent, SnapshotRow, StyleRun};
pub(crate) use state::EngineView;
pub use state::{SnapshotCursor, SnapshotPlacement, SnapshotState, SnapshotUpdate};
pub(crate) use sync::SyncState;

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "snapshot_bench.rs"]
mod bench;
