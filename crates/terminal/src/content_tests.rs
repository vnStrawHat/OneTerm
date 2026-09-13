//! `TerminalContent` over the render state it owns.
//!
//! The five tests `US-0081` rebuilt on the shim are kept and say the same
//! things; what moved is that the tri-state is now visible (`update()`), so the
//! tests that used to infer "nothing was copied" from the damage shape assert it
//! directly.

use super::*;
use crate::backend::GridSize;
use crate::test_engine::{feed, terminal};

fn snapshot(term: &mut Terminal) -> TerminalContent {
    TerminalContent::from(term)
}

#[test]
fn snapshot_has_cells_and_bounds() {
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello\r\nworld");
    let snap = snapshot(&mut term);
    assert_eq!(snap.terminal_bounds.num_cols, 5);
    assert_eq!(snap.terminal_bounds.num_lines, 2);
    assert_eq!(snap.cells.len(), 10);
    assert_eq!(snap.cells[0].cell.c, 'h');
    assert_eq!(snap.cells[5].cell.c, 'w');
}

/// The native view of the same frame: the full viewport as `RenderRow`s, which
/// is what `US-0085` consumes instead of the dense cell vector.
#[test]
fn the_frame_source_is_the_render_state() {
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello\r\nworld");
    let snap = snapshot(&mut term);

    assert_eq!(snap.update(), RenderUpdate::Full);
    assert_eq!(snap.rows().len(), 2, "rows() is always the full viewport");
    assert_eq!(snap.size(), Size { rows: 2, cols: 5 });
    assert_eq!(snap.changed(), &[0, 1]);
    // Row identity is the engine's, and the two translations agree.
    let top = snap.row_id(0).expect("a first row");
    assert_eq!(snap.display_row(top), Some(0));
    assert_eq!(snap.display_row(snap.row_id(1).unwrap()), Some(1));
    assert_eq!(snap.row_id(2), None);
}

#[test]
fn the_snapshot_is_owned_and_outlives_the_engine() {
    let mut term = terminal(GridSize { cols: 4, lines: 1 });
    feed(&mut term, b"ab");
    let snap = snapshot(&mut term);
    // Nothing borrows the engine → the frame is truly owned.
    drop(term);
    assert_eq!(snap.cells.len(), 4);
    assert_eq!(snap.rows()[0].cells.len(), 4);
}

#[test]
fn cursor_visible_default() {
    let mut term = terminal(GridSize { cols: 4, lines: 1 });
    feed(&mut term, b"x");
    let snap = snapshot(&mut term);
    // The cursor is visible at power-on.
    assert!(snap.cursor_visible());
    assert_eq!(snap.cursor.point.column.0, 1);
    assert_eq!(snap.render_cursor().col, 1);
    assert!(snap.modes().show_cursor);
}

#[test]
fn damage_full_on_first_snapshot() {
    // The first frame of a fresh render state must be Full.
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello");
    let snap = snapshot(&mut term);
    assert_eq!(snap.damage, TermDamageInfo::Full);
}

/// TEST-24: a second frame with no new output must not report full damage; at
/// most the cursor line is dirty, and no row is copied at all.
#[test]
fn damage_partial_on_unchanged() {
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello");
    let mut content = TerminalContent::default();
    content.refill(&mut term);
    let cursor_line = content.cursor.point.line.0 as usize;

    content.refill(&mut term);
    assert_eq!(content.update(), RenderUpdate::Unchanged);
    assert!(content.changed().is_empty(), "no row was copied");
    match &content.damage {
        TermDamageInfo::Partial(lines) => {
            assert!(
                lines.iter().all(|line| *line == cursor_line),
                "only the cursor line may be dirty, got {lines:?} (cursor {cursor_line})"
            );
        }
        TermDamageInfo::Full => panic!("unchanged terminal must not report full damage"),
    }
}

/// The compatibility cells are refreshed from the tri-state, not rebuilt: a
/// frame that changed one row must leave the other rows' cells exactly as they
/// were, and must still be correct.
#[test]
fn only_the_changed_rows_are_rebuilt_when_the_viewport_stood_still() {
    let mut term = terminal(GridSize { cols: 6, lines: 3 });
    feed(&mut term, b"aaa\r\nbbb\r\nccc");
    let mut content = TerminalContent::default();
    content.refill(&mut term);
    assert_eq!(content.update(), RenderUpdate::Full);

    // Overwrite the middle row in place: CUP to row 2, column 1.
    feed(&mut term, b"\x1b[2;1HZZZ");
    content.refill(&mut term);

    assert_eq!(content.update(), RenderUpdate::Partial { scrolled: 0 });
    assert_eq!(content.changed(), &[1], "one row copied");
    let text = |line: usize| -> String {
        content.cells[line * 6..(line + 1) * 6]
            .iter()
            .map(|cell| cell.cell.c)
            .collect::<String>()
            .trim_end()
            .to_owned()
    };
    assert_eq!(text(0), "aaa", "an untouched row keeps its cells");
    assert_eq!(text(1), "ZZZ");
    assert_eq!(text(2), "ccc");
    // The points stayed consistent with the rebuild.
    assert_eq!(content.cells[6].point.line.0, 1);
    assert_eq!(content.cells[6].point.column.0, 0);
}

/// A scroll moves every row's grid line, so the reference reported `Full` and
/// the compatibility vector is rebuilt whole. The lines must follow the scroll.
#[test]
fn a_scrollback_move_rebuilds_every_point() {
    let mut term = terminal(GridSize { cols: 4, lines: 2 });
    feed(&mut term, b"one\r\ntwo\r\nthree");
    let mut content = TerminalContent::default();
    content.refill(&mut term);
    assert_eq!(content.cells[0].point.line.0, 0);

    // Negative moves the viewport towards history, which is the sign
    // `TerminalModel::scroll` negates for its callers.
    term.grid_mut().screen_mut().scroll_viewport(-1);
    term.grid_mut().sync_anchors();
    content.refill(&mut term);

    assert_eq!(content.display_offset, 1);
    assert_eq!(content.damage, TermDamageInfo::Full);
    assert_eq!(content.cells[0].point.line.0, -1, "history is negative");
    assert_eq!(content.cells[4].point.line.0, 0);
}

#[test]
fn last_content_line_finds_the_last_written_row() {
    let mut term = terminal(GridSize { cols: 8, lines: 5 });
    feed(&mut term, b"one\r\ntwo\r\n");
    assert_eq!(last_content_line(&term), 1);
}

/// Colours and attributes are converted once per style run; the run boundaries
/// must land on the right columns or a whole run paints in the wrong colour.
#[test]
fn style_runs_reach_the_right_columns() {
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::vte::ansi::{Color as LegacyColor, NamedColor};

    let mut term = terminal(GridSize { cols: 9, lines: 1 });
    feed(&mut term, b"ab\x1b[1;31mcd\x1b[0mef");
    let content = snapshot(&mut term);

    assert!(!content.cells[0].cell.flags.contains(Flags::BOLD));
    for col in 2..4 {
        assert!(
            content.cells[col].cell.flags.contains(Flags::BOLD),
            "column {col} is inside the bold run"
        );
        assert_eq!(
            content.cells[col].cell.fg,
            LegacyColor::Named(NamedColor::Red)
        );
    }
    assert!(!content.cells[4].cell.flags.contains(Flags::BOLD));
    assert_eq!(
        content.cells[4].cell.fg,
        LegacyColor::Named(NamedColor::Foreground)
    );
}
