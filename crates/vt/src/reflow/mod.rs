//! Resize: the two policies, and the column reflow they share.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/reflow-and-resize.md`;
//! the ConPTY contract is `docs/decisions/DEC-0008-local-conpty-grow-resize-keeps-viewport-top.md`
//! and `docs/terminal-backend.md` § 5.3.
//!
//! Replaces Alacritty's `alacritty_terminal/src/grid/resize.rs` **and**
//! `crates/terminal/src/model.rs`'s `resize_keeping_viewport_top` /
//! `conhost_cursor_row`, which parked the alternate grid, installed a
//! placeholder, swapped screens twice and reflowed a scratch grid to guess what
//! conhost had done. The engine owns both screens, so the correction addresses
//! the primary one directly.
//!
//! There is no public tracking-point slice and no public row remap (R-31):
//! everything that has to move is already an entry in the tracked-anchor list,
//! and a consumer that wants its own anchor registers one.

use crate::grid::{Anchors, Pos, Screen, Size};

mod columns;

pub(crate) use columns::measure_rows;

/// Where a resize leaves the content.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum ResizePolicy {
    /// The reference's behaviour, and what a remote PTY expects: a grow pulls
    /// scrollback into the top of the viewport and the cursor moves down with
    /// it. SSH sessions and Unix local shells.
    #[default]
    BottomAnchor,
    /// conhost's behaviour behind ConPTY (`DEC-0008`): the viewport keeps its
    /// top row, the cursor moves to the row conhost addresses, and new rows are
    /// blank at the bottom. Windows local shells.
    KeepViewportTop,
}

/// What one resize did. The `Full` render update a resize implies is derived by
/// `RenderState` from the size it last observed (`damage-and-render-state.md`),
/// not stamped per row here.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct ResizeOutcome {
    /// Whether the column count changed, so the rows were re-laid out.
    pub reflowed: bool,
    /// Rows dropped off the oldest end because the reflow produced more than the
    /// scrollback limit allows (trap 32).
    pub rows_trimmed: u32,
}

/// Resize both screens.
///
/// The order is the design's: the early identity return, the primary screen's
/// columns then rows, the alternate screen without reflow (trap 29), the
/// `KeepViewportTop` correction, the selection, and one `sync_anchors` at the
/// end — after the reflow has read its remapped entries back into the fields.
pub(crate) fn resize(
    primary: &mut Screen,
    alt: &mut Screen,
    size: Size,
    policy: ResizePolicy,
    anchors: &mut Anchors,
) -> ResizeOutcome {
    let size = size.clamped();
    // kitty's first branch: nothing changed, so nothing moved and nothing is
    // damaged.
    if size == primary.size() && size == alt.size() {
        return ResizeOutcome::default();
    }

    // The cached fields are the authority under the scroll primitives and the
    // anchor entries are the authority under a reflow, so the list is brought up
    // to date before it is read. `Screen::scroll_viewport` in particular moves
    // the viewport without writing its entry back.
    primary.sync_anchors(anchors);
    alt.sync_anchors(anchors);

    // Measured before anything moves, and always on the primary screen: the
    // correction applies to it even while a TUI holds the alternate screen.
    let conhost_row = match policy {
        ResizePolicy::KeepViewportTop => Some(measure_rows(primary, size.cols).min(size.rows - 1)),
        ResizePolicy::BottomAnchor => None,
    };
    let pre_offset = primary.scroll_offset();
    let sticky = pre_offset == 0;

    let reflowed = size.cols != primary.cols();
    let mut rows_trimmed = 0;
    if reflowed {
        let outcome = columns::reflow_columns(primary, size.cols, anchors);
        rows_trimmed = outcome.rows_trimmed;
        read_back(primary, anchors, size.cols, outcome.cursor, sticky);
    }
    rows_trimmed += primary.resize_rows(size.rows, anchors);

    // Trap 29: the alternate screen is truncated and padded, never reflowed.
    alt.truncate_columns(size.cols, anchors);
    alt.resize_rows(size.rows, anchors);

    let mut shifted = false;
    if let Some(conhost_row) = conhost_row {
        shifted = keep_viewport_top(primary, conhost_row, anchors);
        primary.set_scroll_offset(pre_offset);
    }
    // Trap 28: a width change invalidates the selection, and so does a
    // correction that moved the screen under it.
    if reflowed || shifted {
        anchors.kill_selection();
    }

    primary.sync_anchors(anchors);
    alt.sync_anchors(anchors);
    ResizeOutcome {
        reflowed,
        rows_trimmed,
    }
}

/// Read the reflow's remapped entries back into the screen's own fields.
///
/// This has to happen before the next `Screen::sync_anchors`, which writes the
/// list from the fields and would otherwise undo the remap. The cursor is taken
/// from the reflow's own answer rather than from the list, because only that one
/// distinguishes "on the last column" from "past it", which is the pending wrap.
fn read_back(
    screen: &mut Screen,
    anchors: &Anchors,
    new_cols: u16,
    cursor: Option<Pos>,
    sticky: bool,
) {
    let last_row = screen.newest();
    let (pos, pending_wrap) = match cursor {
        Some(pos) => {
            // Trap 31: the wrap is re-armed only when the cursor really is past
            // the last column of a row that does not continue.
            let pending = pos.col >= new_cols && !screen.row(pos.row).wrapped();
            let col = pos.col.min(new_cols - 1);
            (Pos { row: pos.row, col }, pending)
        }
        // More text followed the cursor than the scrollback could hold.
        None => (
            Pos {
                row: last_row,
                col: 0,
            },
            false,
        ),
    };
    screen.restate_cursor_after_reflow(pos, pending_wrap);

    let saved = anchors.get(screen.saved_cursor_anchor()).unwrap_or(Pos {
        row: last_row,
        col: 0,
    });
    screen.restate_saved_cursor_after_reflow(saved);

    let top = anchors
        .get(screen.viewport_anchor())
        .map_or(screen.oldest(), |pos| pos.row);
    screen.restate_viewport(top, sticky);
}

/// The `KeepViewportTop` correction: shift the screen by the difference between
/// the row the engine put the cursor on and the row conhost addresses.
///
/// Returns whether anything moved. A positive shift scrolls the whole screen up,
/// which returns the top rows to history and blanks the bottom; a negative shift
/// (a column shrink split rows above the cursor) pulls the split rows back out of
/// history and drops the blank rows below the cursor.
fn keep_viewport_top(primary: &mut Screen, conhost_row: u16, anchors: &mut Anchors) -> bool {
    let shift = i32::from(primary.cursor_row_index()) - i32::from(conhost_row);
    if shift > 0 {
        primary.append_blank_rows(shift as u16, anchors);
    } else if shift < 0 {
        primary.drop_trailing_rows(shift.unsigned_abs() as u16, anchors);
    }
    shift != 0
}

#[cfg(test)]
#[path = "reflow_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reflow_props.rs"]
mod props;
