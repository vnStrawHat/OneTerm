//! `TerminalContent` over the render state it owns.
//!
//! `US-0085` deleted the compatibility surface, so what used to be asserted
//! through the dense `IndexedCell` vector is asserted through `rows()` — the
//! same facts, one conversion closer to the engine. The row-identity and
//! tri-state assertions are unchanged.

use super::*;
use crate::backend::GridSize;
use crate::test_engine::{feed, terminal};

fn snapshot(term: &mut Terminal) -> TerminalContent {
    TerminalContent::from(term)
}

/// The text of one display row, as the view reads it.
fn row_text(content: &TerminalContent, row: usize) -> String {
    let row = &content.rows()[row];
    row.cells
        .iter()
        .map(|cell| match cell.content {
            oneterm_vt::RenderContent::Scalar(scalar) => scalar,
            oneterm_vt::RenderContent::Cluster { start, len } => {
                row.cluster(start, len).first().copied().unwrap_or(' ')
            }
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn the_frame_source_is_the_render_state() {
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello\r\nworld");
    let snap = snapshot(&mut term);

    assert_eq!(snap.update(), RenderUpdate::Full);
    assert_eq!(snap.rows().len(), 2, "rows() is always the full viewport");
    assert_eq!(snap.size(), Size { rows: 2, cols: 5 });
    assert_eq!(snap.changed(), &[0, 1]);
    assert_eq!(row_text(&snap, 0), "hello");
    assert_eq!(row_text(&snap, 1), "world");
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
    assert_eq!(snap.rows()[0].cells.len(), 4);
    assert_eq!(row_text(&snap, 0), "ab");
}

#[test]
fn cursor_visible_default() {
    let mut term = terminal(GridSize { cols: 4, lines: 1 });
    feed(&mut term, b"x");
    let snap = snapshot(&mut term);
    // The cursor is visible at power-on.
    assert!(snap.cursor_visible());
    assert_eq!(snap.render_cursor().col, 1);
    assert_eq!(snap.render_cursor().row, Some(0));
    assert!(snap.modes().show_cursor);
    assert_ne!(snap.cursor_shape(), oneterm_vt::CursorShape::Hidden);
}

#[test]
fn the_first_update_is_full() {
    // The first frame of a fresh render state must be Full.
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello");
    let snap = snapshot(&mut term);
    assert_eq!(snap.update(), RenderUpdate::Full);
}

/// TEST-24: a second frame with no new output copies no row at all — which is
/// what lets the element skip layout and paint entirely.
#[test]
fn an_unchanged_frame_copies_nothing() {
    let mut term = terminal(GridSize { cols: 5, lines: 2 });
    feed(&mut term, b"hello");
    let mut content = TerminalContent::default();
    content.refill(&mut term);

    content.refill(&mut term);
    assert_eq!(content.update(), RenderUpdate::Unchanged);
    assert!(content.changed().is_empty(), "no row was copied");
    assert_eq!(
        content.rows().len(),
        2,
        "rows() still holds the full viewport"
    );
}

/// A frame that changed one row must leave the other rows exactly as they were,
/// and must still be correct.
#[test]
fn only_the_changed_rows_are_copied_when_the_viewport_stood_still() {
    let mut term = terminal(GridSize { cols: 6, lines: 3 });
    feed(&mut term, b"aaa\r\nbbb\r\nccc");
    let mut content = TerminalContent::default();
    content.refill(&mut term);
    assert_eq!(content.update(), RenderUpdate::Full);
    let seq_before = content.rows()[0].seq;

    // Overwrite the middle row in place: CUP to row 2, column 1.
    feed(&mut term, b"\x1b[2;1HZZZ");
    content.refill(&mut term);

    assert_eq!(content.update(), RenderUpdate::Partial { scrolled: 0 });
    assert_eq!(content.changed(), &[1], "one row copied");
    assert_eq!(row_text(&content, 0), "aaa", "an untouched row is kept");
    assert_eq!(row_text(&content, 1), "ZZZ");
    assert_eq!(row_text(&content, 2), "ccc");
    assert_eq!(
        content.rows()[0].seq,
        seq_before,
        "an untouched row keeps its sequence number, which is the plan cache's key"
    );
}

/// A scroll moves every row's display index but not its identity, so a
/// `RowId`-keyed cache shifts instead of rebuilding.
#[test]
fn a_scrollback_move_reports_a_delta_and_keeps_row_identity() {
    let mut term = terminal(GridSize { cols: 4, lines: 2 });
    feed(&mut term, b"one\r\ntwo\r\nthree");
    let mut content = TerminalContent::default();
    content.refill(&mut term);
    let was_on_top = content.row_id(0).expect("a first row");
    let was_on_top_text = row_text(&content, 0);

    // Negative moves the viewport towards history, which is the sign
    // `TerminalModel::scroll` negates for its callers.
    term.grid_mut().screen_mut().scroll_viewport(-1);
    term.grid_mut().sync_anchors();
    content.refill(&mut term);

    assert_eq!(content.scroll_offset(), 1);
    assert_eq!(content.update(), RenderUpdate::Partial { scrolled: -1 });
    assert_eq!(
        content.display_row(was_on_top),
        Some(1),
        "the row that was on top moved down one, keeping its id"
    );
    assert_eq!(
        row_text(&content, 1),
        was_on_top_text,
        "and its content moved with it"
    );
}

#[test]
fn last_content_row_finds_the_last_written_row() {
    let mut term = terminal(GridSize { cols: 8, lines: 5 });
    feed(&mut term, b"one\r\ntwo\r\n");
    assert_eq!(last_content_row(&term), 1);
}

/// The answer plus the cells the scan examined (`US-0092`).
fn last_content_row_cost(term: &Terminal) -> (usize, usize) {
    CELLS_EXAMINED.with(|n| n.set(0));
    let row = last_content_row(term);
    (row, CELLS_EXAMINED.with(|n| n.get()))
}

/// `US-0092` counted work: on the idle screen — a prompt on row 0 and blanks
/// below, which is `last_content_row`'s worst case and the common case —
/// doubling the column count must not change the work. Before the `occ` skip it
/// doubled, because every blank cell of every blank row cost three interner
/// lookups.
#[test]
fn last_content_row_cost_follows_the_content_not_the_viewport() {
    let mut narrow = terminal(GridSize {
        cols: 40,
        lines: 45,
    });
    feed(&mut narrow, b"$ ");
    let mut wide = terminal(GridSize {
        cols: 80,
        lines: 45,
    });
    feed(&mut wide, b"$ ");

    let (row, narrow_cells) = last_content_row_cost(&narrow);
    assert_eq!(row, 0);
    let (row, wide_cells) = last_content_row_cost(&wide);
    assert_eq!(row, 0);
    assert_eq!(
        narrow_cells, wide_cells,
        "doubling the columns must not change the work"
    );
    assert_eq!(
        wide_cells, 2,
        "the two cells the prompt actually occupies, not 45 x 80"
    );
}

/// The six cases the skip must not get wrong. `occ` over-approximates, so a row
/// that was written and then cleared is still examined — and must still be
/// reported as blank.
#[test]
fn last_content_row_pins_the_blank_definition() {
    let mut blank = terminal(GridSize { cols: 8, lines: 5 });
    feed(&mut blank, b"");
    assert_eq!(last_content_row(&blank), 0, "an all-blank screen");

    let mut last = terminal(GridSize { cols: 8, lines: 3 });
    feed(&mut last, b"a\r\nb\r\nc");
    assert_eq!(last_content_row(&last), 2, "content on the last row");

    let mut first = terminal(GridSize { cols: 8, lines: 5 });
    feed(&mut first, b"a");
    assert_eq!(last_content_row(&first), 0, "content on row 0 only");

    // A wide glyph: its second column is a `WideSpacer`, which is not blank.
    let mut wide = terminal(GridSize { cols: 8, lines: 4 });
    feed(&mut wide, "a\r\n\r\n日".as_bytes());
    assert_eq!(last_content_row(&wide), 2, "a wide pair is content");

    // A row whose only content is an OSC 8 link on a space cell.
    let mut link = terminal(GridSize { cols: 8, lines: 4 });
    feed(
        &mut link,
        b"a\r\n\r\n\x1b]8;;https://a.test\x07 \x1b]8;;\x07",
    );
    assert_eq!(last_content_row(&link), 2, "a hyperlink cell is content");

    // Written then cleared: `occ` still says the row was touched, so the skip
    // must not fire and the row must read as blank again.
    let mut cleared = terminal(GridSize { cols: 8, lines: 4 });
    feed(&mut cleared, b"a\r\n\r\ngone\x1b[2K");
    assert_eq!(
        last_content_row(&cleared),
        0,
        "a cleared row is blank again"
    );
    assert!(
        last_content_row_cost(&cleared).1 > 0,
        "and it was actually examined, not skipped"
    );
}

/// Colours and attributes arrive as runs of resolved values; the run boundaries
/// must land on the right columns or a whole run paints in the wrong colour.
#[test]
fn style_runs_reach_the_right_columns() {
    let mut term = terminal(GridSize { cols: 9, lines: 1 });
    feed(&mut term, b"ab\x1b[1;31mcd\x1b[0mef");
    let content = snapshot(&mut term);
    let row = &content.rows()[0];

    let style = |col: usize| row.style_of(&row.cells[col]);
    assert!(!style(0).attrs.contains(Attrs::BOLD));
    for col in 2..4 {
        assert!(
            style(col).attrs.contains(Attrs::BOLD),
            "column {col} is inside the bold run"
        );
        assert_eq!(style(col).fg, Color::Named(NamedColor::Red));
    }
    assert!(!style(4).attrs.contains(Attrs::BOLD));
    assert_eq!(style(4).fg, Color::Named(NamedColor::Foreground));
    // Three runs, not nine cells' worth of style.
    assert_eq!(row.runs.len(), 3);
}

/// The hyperlink strings are resolved under the lock and reachable by id, which
/// is what the view keys its underline and its click target on.
#[test]
fn hyperlinks_are_reachable_by_id() {
    let mut term = terminal(GridSize { cols: 8, lines: 1 });
    feed(&mut term, b"\x1b]8;id=x1;https://a.test\x07ab\x1b]8;;\x07");
    let content = snapshot(&mut term);
    let row = &content.rows()[0];
    let id = row.cells[0].hyperlink.expect("a link on the first cell");
    assert_eq!(row.cells[1].hyperlink, Some(id), "one run, one id");
    assert!(row.cells[2].hyperlink.is_none());
    let link = content.hyperlink(id).expect("the strings");
    assert_eq!(link.uri.as_ref(), "https://a.test");
}
