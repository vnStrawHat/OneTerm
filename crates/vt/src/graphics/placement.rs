//! Placing a finished image on the grid, and the row-derived release sweep.
//!
//! Design:
//! <https://github.com/vnStrawHat/OneTerm/blob/main/docs/spec-intakes/IN-0029-vt-engine/low-level-design/graphics.md>

use std::sync::Arc;

use crate::cell::Cell;
use crate::event::{EventBatch, VtEvent};
use crate::grid::{AnchorKind, RowFlags, RowId, ScrollReport, TerminalGrid};
use crate::intern::{Extras, ExtrasId, GraphicId};
use crate::terminal::State;

use super::sixel::DecodedSixel;
use super::{GraphicData, MAX_PLACEMENTS, Placement, VIRTUAL_CELL};

/// Place a finished image at the cursor, the way DEC terminals and Windows
/// conhost do (Sixel scrolling mode).
///
/// Pixels are measured in [`VIRTUAL_CELL`] units; every covered cell gets the
/// image's id and nothing else; the cursor walks down `bands * 6 / 20` rows
/// through the ordinary line feed — so the scroll region, the scrollback and
/// the damage all behave as they do for text — and **keeps its column**. Image
/// rows below the final cursor row are placed without further scrolling and are
/// clipped at the bottom of the screen.
///
/// The 10x20 virtual cell and the `bands * 6 / 20` rule are the conhost
/// agreement: they are why the prompt lands below the image rather than inside
/// it when conhost issues its absolute `CUP`. Do not "improve" either without a
/// fresh ConPTY capture.
///
/// Returns the scroll reports the line feeds produced, for the caller to turn
/// into events — the engine's one reporting path lives on the dispatch handler.
pub(crate) fn place(state: &mut State, image: DecodedSixel) -> Vec<ScrollReport> {
    let DecodedSixel {
        width,
        height,
        rgba,
        cursor_rows,
    } = image;
    let id = state.graphics.next_id();
    let cursor = state.grid.screen().cursor().pos;
    let screen_cols = state.grid.screen().cols();
    // Clipped on the right, never wrapped.
    let cols = cells_for(width, VIRTUAL_CELL.0).min(screen_cols.saturating_sub(cursor.col));
    let rows = cells_for(height, VIRTUAL_CELL.1).max(1);

    // R-21: **one** interned extras entry for the whole image. A covered cell
    // that already carried a hyperlink keeps it and gets its own entry, which
    // is rare enough not to threaten the id space.
    let graphic_only = state.interner.extras(&Extras {
        hyperlink: None,
        graphic: Some(id),
    });

    evict_oldest(state);
    let anchor = state
        .grid
        .anchors_mut()
        .register(AnchorKind::Graphic(id), cursor);
    state.graphics.placements.push(Placement {
        id,
        anchor,
        cols,
        rows,
        pixel_size: (width, height),
    });

    let mut reports = Vec::new();
    for row in 0..rows.max(cursor_rows.saturating_add(1)) {
        let at = state.grid.screen().cursor().pos.row;
        let line = if row <= cursor_rows {
            at
        } else {
            at + u64::from(row - cursor_rows)
        };
        if row < rows && state.grid.screen().index_of(line).is_some() {
            stamp(state, line, cursor.col, cols, id, graphic_only);
        }
        if row < cursor_rows {
            reports.extend(state.grid.linefeed());
        }
    }

    // After the cells, so nothing can observe pixels for an image that is not
    // on the grid yet.
    state.graphics.pending.push(Arc::new(GraphicData {
        id,
        width,
        height,
        rgba,
    }));
    reports
}

/// Pixels to cells, rounding up, and saturating rather than wrapping: both
/// dimensions are clamped to `MAX_DIMENSION`, so the `u16` always fits.
fn cells_for(pixels: u32, cell: u16) -> u16 {
    u16::try_from(pixels.div_ceil(u32::from(cell))).unwrap_or(u16::MAX)
}

/// Write the image's id into one row's covered cells, leaving their text and
/// style alone.
///
/// The write is [`RowMut::repair`](crate::grid::RowMut::repair), not `set`:
/// this is not an overwrite, so it must not clear the row's wrap flag the way
/// a write to the last column does (deviation G1).
fn stamp(state: &mut State, line: RowId, col: u16, cols: u16, id: GraphicId, only: ExtrasId) {
    let State { grid, interner, .. } = state;
    let mut row = grid.screen_mut().row_mut(line);
    for offset in 0..cols {
        let column = col.saturating_add(offset);
        let Some(cell) = row.cells().get(column as usize).copied() else {
            break;
        };
        let extras = if cell.extras_id() == ExtrasId::NONE {
            only
        } else {
            let mut merged = *interner.resolve_extras(cell.extras_id());
            merged.graphic = Some(id);
            interner.extras(&merged)
        };
        row.repair(column, cell.with_extras(extras));
    }
    row.mark_graphic();
}

/// Release placements whose content is gone.
///
/// A placement dies when its anchor died — a scroll blanked its row, a trim
/// dropped it out of history, or a reflow could not carry it — or when **no**
/// row of its extent still carries [`RowFlags::HAS_GRAPHIC`], which is what
/// `Row::reset` clears and therefore what `CSI 2 J`, `clear`, an alternate
/// screen wipe and every TUI repaint reach.
///
/// A row that was overwritten with text keeps the flag until it is reset, which
/// is a false positive the design allows: the image is anchored to its cell, so
/// the renderer still paints all of it from its placement. That is the known
/// limitation of cell-anchored placement, not an oversight.
pub(crate) fn sweep(state: &mut State) {
    let mut index = 0;
    while index < state.graphics.placements.len() {
        let placement = state.graphics.placements[index];
        if is_live(&state.grid, placement) {
            index += 1;
            continue;
        }
        state.graphics.placements.remove(index);
        state.grid.anchors_mut().release(placement.anchor);
        state.graphics.released.push(placement.id);
    }
}

fn is_live(grid: &TerminalGrid, placement: Placement) -> bool {
    let Some(pos) = grid.anchors().get(placement.anchor) else {
        return false;
    };
    let screen = grid.screen_of(pos.row);
    (0..placement.rows).any(|offset| {
        screen
            .row(pos.row + u64::from(offset))
            .flags()
            .contains(RowFlags::HAS_GRAPHIC)
    })
}

/// Hand every release since the last drain to the embedder, exactly once each.
///
/// A queue rather than a direct push because `resize` releases placements too
/// and has no event batch to push into; `feed` is the one place that delivers.
pub(crate) fn drain_released(state: &mut State, out: &mut EventBatch) {
    for id in state.graphics.released.drain(..) {
        out.push(VtEvent::GraphicReleased(id));
    }
}

/// The oldest placement goes when the table is full, so a stream that emits an
/// image per line frees the view's textures instead of leaking them.
fn evict_oldest(state: &mut State) {
    while state.graphics.placements.len() >= MAX_PLACEMENTS {
        let placement = state.graphics.placements.remove(0);
        state.grid.anchors_mut().release(placement.anchor);
        state.graphics.released.push(placement.id);
    }
}

/// The debug invariant: inside a live placement's extent, every `GraphicId` a
/// cell carries resolves to a live placement.
///
/// Scoped to those extents on purpose. A cell *outside* every extent can hold a
/// stale id — an `SD` or `IL` can push part of an image below its own anchor's
/// extent — and that is inert rather than wrong: the painter resolves the id
/// through the placement table and paints nothing when it is gone.
///
/// ponytail: O(placements x rows x cols) per feed in debug builds; make it
/// incremental if a debug session with hundreds of live images ever gets slow.
pub(crate) fn assert_integrity(state: &State) {
    if !cfg!(debug_assertions) || state.graphics.placements.is_empty() {
        return;
    }
    for placement in &state.graphics.placements {
        let Some(pos) = state.grid.anchors().get(placement.anchor) else {
            debug_assert!(false, "live placement {:?} has a dead anchor", placement.id);
            continue;
        };
        let screen = state.grid.screen_of(pos.row);
        for offset in 0..placement.rows {
            let row = screen.row(pos.row + u64::from(offset));
            if !row.flags().contains(RowFlags::HAS_GRAPHIC) {
                continue;
            }
            for cell in row.cells() {
                debug_assert!(
                    resolves(state, *cell),
                    "a cell references a graphic with no live placement"
                );
            }
        }
    }
}

fn resolves(state: &State, cell: Cell) -> bool {
    match state.interner.resolve_extras(cell.extras_id()).graphic {
        Some(id) => state.graphics.placement(id).is_some(),
        None => true,
    }
}
