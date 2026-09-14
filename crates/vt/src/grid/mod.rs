//! The screen: rows, scrollback, the viewport and the tracked anchors.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`,
//! contract: `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md`.
//!
//! A [`RowId`] names a **position** in the output stream, not a piece of content,
//! and the ring index *is* the id (`slot = id & mask`). Content therefore moves
//! between fixed ids whenever a TUI repaints, which is why a consumer never
//! stores a `RowId` and assumes the content stayed: it registers an
//! `Anchor` and reads it back.
//!
//! The viewport is a distance from the newest row rather than an absolute top,
//! so "follow the output" is expressible as `offset == 0` and there are no
//! negative row indices anywhere.

use std::ops::{Add, Sub};

mod anchor;
mod row;
mod screen;
mod terminal_grid;

pub use anchor::{Anchor, AnchorId, AnchorKind, Anchors};
/// The content hints one cell implies, for the reflow's single-pass layout.
pub(crate) use row::flags_for;
pub use row::{Row, RowFlags, RowHeader, RowMut, RowRef, SeqNo};
pub use screen::{
    Charset, Cursor, CursorOrigin, DisplayClear, LineClear, PrintMode, RowsScrolled, Screen,
    ScreenKind, ScrollReport, TabStops,
};
pub use terminal_grid::TerminalGrid;

/// Hard cap on the viewport height (N-11). The ring is sized once from
/// `scrollback_limit + MAX_ROWS`, so a resize can never invalidate the mask.
pub(crate) const MAX_ROWS: u16 = 1024;
/// Hard cap on the viewport width (N-11).
pub(crate) const MAX_COLS: u16 = 2048;
/// Hard cap on the configured scrollback depth.
pub const SCROLLBACK_MAX: u32 = 1_000_000;
/// The shipped default, unchanged from the engine being replaced.
pub const DEFAULT_SCROLLBACK: u32 = 10_000;

/// A position in the output stream.
///
/// Allocated from one terminal-wide space shared by both screens, so an id is
/// never ambiguous and never reused while its row is live. It is never reset:
/// `RIS`, `ED 2` and `ED 3` clear content and leave ids alone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct RowId(pub u64);

impl RowId {
    /// Where the primary screen's run of the id space starts.
    pub const PRIMARY_ORIGIN: RowId = RowId(0);
    /// Where the alternate screen's run starts.
    ///
    /// The two screens grow independently — the alternate scrolls while the
    /// primary is frozen, and `US-0077`'s `KeepViewportTop` corrects the primary
    /// *while* the alternate is active — so one shared counter cannot keep both
    /// runs contiguous. Splitting the `u64` into two lanes keeps every property
    /// the design asks for (one id space, ids unambiguous, each screen
    /// contiguous, the two runs disjoint) without a gap in either run.
    pub const ALT_ORIGIN: RowId = RowId(1 << 63);

    /// Distance from `earlier` to `self`, saturating rather than wrapping on an
    /// out-of-order pair.
    pub fn distance(self, earlier: RowId) -> u64 {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<u64> for RowId {
    type Output = RowId;

    fn add(self, n: u64) -> RowId {
        RowId(self.0.saturating_add(n))
    }
}

impl Sub<u64> for RowId {
    type Output = RowId;

    fn sub(self, n: u64) -> RowId {
        RowId(self.0.saturating_sub(n))
    }
}

/// One grid position: an absolute row and a column.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Pos {
    pub row: RowId,
    pub col: u16,
}

/// A viewport size in cells.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Size {
    /// Clamp to the engine's hard caps, and to at least one cell: every caller
    /// of a resize is ultimately a remote host, so the bounds are enforced here
    /// rather than trusted.
    pub fn clamped(self) -> Size {
        Size {
            rows: self.rows.clamp(1, MAX_ROWS),
            cols: self.cols.clamp(1, MAX_COLS),
        }
    }
}

/// What the caller can see, derived from the internal scroll offset.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Viewport {
    /// The first visible row.
    pub top: RowId,
    pub rows: u16,
    pub cols: u16,
}

/// `DECSTBM`, in viewport coordinates. `bottom` is exclusive.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ScrollRegion {
    pub top: u16,
    pub bottom: u16,
}

impl ScrollRegion {
    pub fn full(rows: u16) -> ScrollRegion {
        ScrollRegion {
            top: 0,
            bottom: rows,
        }
    }

    pub fn height(self) -> u16 {
        self.bottom.saturating_sub(self.top)
    }

    pub fn contains(self, index: u16) -> bool {
        index >= self.top && index < self.bottom
    }
}

#[cfg(test)]
#[path = "grid_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "grid_props.rs"]
mod props;
