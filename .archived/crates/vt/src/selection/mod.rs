//! Selection: the four kinds, the range they resolve to, and the text they copy.
//!
//! Replaces Alacritty's `alacritty_terminal/src/selection.rs` and the four call
//! sites of its `Selection::rotate`. **Both endpoints are tracked anchors**
//! ([`crate::grid::Anchors`]), so a selection moves with its content through a
//! scroll, an `IL`/`DL` repaint and a reflow by the one mechanism every
//! row-moving primitive already drives. Two consequences the reference cannot
//! express:
//!
//! * a selection inside a `tmux` pane survives the pane's repaint, where a
//!   viewport-relative rotation loses it;
//! * scrolling the viewport does not touch the selection at all, because the
//!   range is in [`RowId`] space rather than in viewport space.
//!
//! The operations are written against [`TerminalGrid`]; the `selection_*`
//! methods on [`crate::Terminal`] are one-line wrappers around them.

// Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md`,
// contract: `docs/decisions/DEC-0015-absolute-row-ids-and-incremental-render-state.md`
// clause 1.

use crate::cell::CellWidth;
use crate::grid::{AnchorId, AnchorKind, Anchors, Pos, RowId, Screen, TerminalGrid};
use crate::intern::Interner;

mod expand;
mod text;

/// The reference's default escape set, preserved because word selection is
/// user-visible behaviour and OneTerm never overrode it.
///
/// It is a [`crate::Config`] field; this is what [`crate::Config::default`]
/// sets it to.
pub(crate) const SEMANTIC_ESCAPE_CHARS: &str = ",│`|:\"' ()[]{}<>\t";

/// How a drag expands into a range.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SelectionKind {
    /// Exactly the cells the drag covered.
    Simple,
    /// A rectangle.
    Block,
    /// The word under the anchor, or the bracket matching it.
    Semantic,
    /// Whole logical lines, across wrapped continuations.
    Lines,
}

/// Which half of a cell the pointer was on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    /// The left half: the selection starts at this cell.
    Left,
    /// The right half: the selection starts after this cell.
    Right,
}

/// A resolved selection, **both ends inclusive**, in absolute row space.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SelectionRange {
    /// The earlier end, in absolute row space.
    pub start: Pos,
    /// The later end, in absolute row space. Inclusive.
    pub end: Pos,
    /// Whether the range is a rectangle rather than a run of whole lines.
    pub is_block: bool,
}

/// The operations that invalidate a selection without moving a row.
///
/// The row-**moving** half of the design's invalidation matrix needs no entry
/// here and no code anywhere: `SU`, `SD`, `IL`, `DL`, `RI`, a scroll into
/// history and a resize already drive `Anchors::shift_region`,
/// `Anchors::trim` and `Anchors::remap`, and an anchor that dies makes
/// [`Selection::to_range`] return `None` — which is the matrix's "cleared".
/// `scroll_viewport` appears nowhere for the same reason: the range is in row
/// space, so a viewport scroll cannot touch it.
///
/// Each variant is answered against the grid **before** the operation runs:
/// every rule is about the rows the operation is going to blank, and the cursor
/// row is the input.
/// Marked `#[non_exhaustive]`: an embedder constructs these to ask the
/// question and never matches on them, so a new erase form is a patch release.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum Invalidation {
    /// `EL 0` / `EL 1` / `EL 2`: the cursor row.
    EraseLine,
    /// `ED 0`: the cursor row down to the bottom of the screen.
    EraseBelow,
    /// `ED 1`: the top of the screen down to the cursor row.
    EraseAbove,
    /// `ED 2`.
    EraseScreen,
    /// `ED 3`: history.
    EraseHistory,
    /// `CSI ? 1049 h/l`.
    SwapAlt,
    /// `RIS`.
    Reset,
}

/// One endpoint: where it is, and which half of that cell the pointer was on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Endpoint {
    pos: Pos,
    side: Side,
}

/// An active selection.
///
/// Owns two entries in the engine's anchor list, which is why
/// [`Selection::release`] consumes `self`: the entries must be given back
/// exactly once, and the type system says so.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Selection {
    kind: SelectionKind,
    start: AnchorId,
    end: AnchorId,
    start_side: Side,
    end_side: Side,
}

impl Selection {
    /// Begin a drag. `Terminal::selection_start`.
    pub fn new(grid: &mut TerminalGrid, kind: SelectionKind, pos: Pos, side: Side) -> Selection {
        let anchors = grid.anchors_mut();
        let selection = Selection {
            kind,
            start: anchors.register(AnchorKind::SelectionStart, pos),
            end: anchors.register(AnchorKind::SelectionEnd, pos),
            start_side: side,
            end_side: side,
        };
        debug_assert_no_leak(anchors);
        selection
    }

    /// The whole of the active screen, history included.
    /// `Terminal::select_all`.
    pub fn all(grid: &mut TerminalGrid) -> Selection {
        let (start, end) = {
            let screen = grid.screen();
            (
                Pos {
                    row: screen.oldest(),
                    col: 0,
                },
                Pos {
                    row: screen.newest(),
                    col: screen.cols().saturating_sub(1),
                },
            )
        };
        let mut selection = Selection::new(grid, SelectionKind::Simple, start, Side::Left);
        selection.update(grid, end, Side::Right);
        selection
    }

    /// Move the end of the drag. `Terminal::selection_update`.
    pub fn update(&mut self, grid: &mut TerminalGrid, pos: Pos, side: Side) {
        grid.anchors_mut().set(self.end, pos);
        self.end_side = side;
    }

    /// Give both anchors back. `Terminal::selection_clear`.
    pub fn release(self, grid: &mut TerminalGrid) {
        let anchors = grid.anchors_mut();
        anchors.release(self.start);
        anchors.release(self.end);
    }

    /// The cells this selection covers, or `None` when it is empty or an
    /// endpoint's content is gone. `Terminal::selection_range`.
    ///
    /// O(1) for `Simple` and `Block`, and allocation-free for every kind: this
    /// is the property `has_selection()` relies on (PERF-14).
    pub fn to_range(&self, grid: &TerminalGrid, escape_chars: &str) -> Option<SelectionRange> {
        let anchors = grid.anchors();
        let mut start = Endpoint {
            pos: anchors.get(self.start)?,
            side: self.start_side,
        };
        let mut end = Endpoint {
            pos: anchors.get(self.end)?,
            side: self.end_side,
        };
        if start.pos > end.pos {
            std::mem::swap(&mut start, &mut end);
        }
        // Each screen's rows live in its own run of the id space, so the start
        // row says which screen this selection is on without a discriminant.
        let screen = grid.screen_of(start.pos.row);
        start.pos = clamp(screen, start.pos);
        end.pos = clamp(screen, end.pos);

        match self.kind {
            SelectionKind::Simple => range_simple(start, end, screen.cols()),
            SelectionKind::Block => range_block(start, end),
            // Never empty, by the design's table.
            SelectionKind::Semantic => Some(expand::range_semantic(
                screen,
                start.pos,
                end.pos,
                escape_chars,
            )),
            SelectionKind::Lines => Some(expand::range_lines(screen, start.pos, end.pos)),
        }
    }

    /// The selected text. `Terminal::selection_text`.
    pub fn text(
        &self,
        grid: &TerminalGrid,
        interner: &Interner,
        escape_chars: &str,
    ) -> Option<String> {
        let range = self.to_range(grid, escape_chars)?;
        let screen = grid.screen_of(range.start.row);
        Some(text::selection_text(screen, interner, self.kind, range))
    }

    /// The design's invalidation matrix, stated once.
    ///
    /// `true` means the caller must clear this selection.
    pub fn invalidated_by(&self, grid: &TerminalGrid, op: Invalidation) -> bool {
        let Some((top, bottom)) = self.row_span(grid.anchors()) else {
            // An endpoint's content is already gone.
            return true;
        };
        let screen = grid.screen_of(top);
        let cursor = screen.cursor().pos.row;
        // Each erase names a range of rows; the rule is always "does the
        // selection meet it". Both bounds are stated even where one cannot be
        // crossed, because the asymmetry is exactly how `ED 1` came to clear a
        // selection lying wholly in history.
        match op {
            Invalidation::EraseLine => top <= cursor && cursor <= bottom,
            Invalidation::EraseBelow => bottom >= cursor && top <= screen.newest(),
            Invalidation::EraseAbove => top <= cursor && bottom >= screen.screen_top(),
            Invalidation::EraseHistory => top < screen.screen_top(),
            Invalidation::EraseScreen | Invalidation::SwapAlt | Invalidation::Reset => true,
        }
    }

    /// The rows the two anchors sit on, ordered. `None` once either is dead.
    fn row_span(&self, anchors: &Anchors) -> Option<(RowId, RowId)> {
        let start = anchors.get(self.start)?.row;
        let end = anchors.get(self.end)?.row;
        Some(if start <= end {
            (start, end)
        } else {
            (end, start)
        })
    }
}

impl SelectionRange {
    /// Whether one position lies inside the range.
    pub fn contains(&self, pos: Pos) -> bool {
        self.start.row <= pos.row
            && self.end.row >= pos.row
            && (self.start.col <= pos.col || (self.start.row != pos.row && !self.is_block))
            && (self.end.col >= pos.col || (self.end.row != pos.row && !self.is_block))
    }

    /// Whether the **cell** at `pos` paints as selected.
    ///
    /// Keeps the reference's two quirks: a block-shaped cursor sitting exactly
    /// on a corner of the selection is not inverted, and a selected
    /// `WideSpacer` pulls in the `Wide` cell to its left — the direction runs
    /// from the spacer to the glyph, not the other way, so selecting only the
    /// `Wide` cell leaves its spacer unpainted. `block_cursor` is `Some(pos)`
    /// when a block-shaped cursor is at that position and `None` for every
    /// other cursor shape.
    pub fn contains_cell(&self, screen: &Screen, pos: Pos, block_cursor: Option<Pos>) -> bool {
        if block_cursor == Some(pos) && self.is_corner(pos) {
            return false;
        }
        if self.contains(pos) {
            return true;
        }
        screen.row(pos.row).cell(pos.col).width() == CellWidth::Wide
            && self.contains(Pos {
                row: pos.row,
                col: pos.col.saturating_add(1),
            })
    }

    fn is_corner(&self, pos: Pos) -> bool {
        self.start == pos
            || self.end == pos
            || (self.is_block
                && ((self.start.row == pos.row && self.end.col == pos.col)
                    || (self.end.row == pos.row && self.start.col == pos.col)))
    }
}

/// Fractional pointer coordinates to a grid position and a side.
///
/// The hit test lives here rather than in a view, so nothing above the engine
/// does `line + display_offset` arithmetic. Out of range clamps rather than
/// producing a position outside the live rows.
pub(crate) fn hit_test(grid: &TerminalGrid, viewport_row: f32, col: f32) -> (Pos, Side) {
    let viewport = grid.screen().viewport();
    let index = (viewport_row.max(0.0) as u32).min(u32::from(viewport.rows.saturating_sub(1)));
    let col = col.max(0.0);
    let column = (col as u32).min(u32::from(viewport.cols.saturating_sub(1))) as u16;
    let side = if col.fract() < 0.5 {
        Side::Left
    } else {
        Side::Right
    };
    (
        Pos {
            row: viewport.top + u64::from(index),
            col: column,
        },
        side,
    )
}

fn clamp(screen: &Screen, pos: Pos) -> Pos {
    Pos {
        row: pos.row.clamp(screen.oldest(), screen.newest()),
        col: pos.col.min(screen.cols().saturating_sub(1)),
    }
}

/// The design's `Simple` row: drop the last cell when the drag ended on a
/// cell's left half, drop the first when it started on a right half.
fn range_simple(mut start: Endpoint, mut end: Endpoint, cols: u16) -> Option<SelectionRange> {
    if is_empty_simple(start, end) {
        return None;
    }
    if end.side == Side::Left && start.pos != end.pos {
        if end.pos.col == 0 {
            end.pos.col = cols.saturating_sub(1);
            end.pos.row = end.pos.row - 1;
        } else {
            end.pos.col -= 1;
        }
    }
    if start.side == Side::Right && start.pos != end.pos {
        start.pos.col += 1;
        if start.pos.col == cols {
            start.pos.col = 0;
            start.pos.row = start.pos.row + 1;
        }
    }
    // Correction over the reference, which builds the range anyway: dragging
    // from the right half of a row's last cell to the left half of the next
    // row's first cell covers no whole cell, and `range_simple` there returns
    // an inverted range its own `SelectionRange::new` would assert on.
    if start.pos > end.pos {
        return None;
    }
    Some(SelectionRange {
        start: start.pos,
        end: end.pos,
        is_block: false,
    })
}

fn is_empty_simple(start: Endpoint, end: Endpoint) -> bool {
    start == end
        || (start.side == Side::Right
            && end.side == Side::Left
            && start.pos.row == end.pos.row
            && start.pos.col.saturating_add(1) == end.pos.col)
}

/// The design's `Block` row: normalise to top-left / bottom-right by swapping
/// **columns and sides only**. The rows were ordered by the caller.
fn range_block(mut start: Endpoint, mut end: Endpoint) -> Option<SelectionRange> {
    if is_empty_block(start, end) {
        return None;
    }
    if start.pos.col > end.pos.col {
        std::mem::swap(&mut start.side, &mut end.side);
        std::mem::swap(&mut start.pos.col, &mut end.pos.col);
    }
    if end.side == Side::Left && start.pos != end.pos && end.pos.col > 0 {
        end.pos.col -= 1;
    }
    if start.side == Side::Right && start.pos != end.pos {
        start.pos.col += 1;
    }
    // The same correction as `range_simple`: a one-column block whose end could
    // not step left leaves the columns crossed.
    if start.pos.col > end.pos.col {
        return None;
    }
    Some(SelectionRange {
        start: start.pos,
        end: end.pos,
        is_block: true,
    })
}

fn is_empty_block(start: Endpoint, end: Endpoint) -> bool {
    (start.pos.col == end.pos.col && start.side == end.side)
        || (start.pos.col.saturating_add(1) == end.pos.col
            && start.side == Side::Right
            && end.side == Side::Left)
        || (end.pos.col.saturating_add(1) == start.pos.col
            && start.side == Side::Left
            && end.side == Side::Right)
}

/// One selection, two anchors: the design's anchor-leak check.
fn debug_assert_no_leak(anchors: &Anchors) {
    if !cfg!(debug_assertions) {
        return;
    }
    let live = anchors
        .iter()
        .filter(|(_, anchor)| {
            anchor.alive
                && matches!(
                    anchor.kind,
                    AnchorKind::SelectionStart | AnchorKind::SelectionEnd
                )
        })
        .count();
    debug_assert!(live <= 2, "a selection leaked anchors: {live} live entries");
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "selection_props.rs"]
mod props;
