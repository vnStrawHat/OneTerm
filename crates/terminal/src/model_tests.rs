//! `TerminalModel` over the new engine.
//!
//! The ten ConPTY resize tests this file used to hold were pinned against the
//! engine being replaced (R-44) and retired with the fork at `US-0087`; the
//! cell-exact resize contract is now `oneterm_vt::reflow`'s own. What is left
//! here is what the *adapter* owns: that the model's operations reach the
//! engine and come back in the coordinates the view expects.

use oneterm_vt::SelectionKind;

use super::{ResizePolicy, TerminalModel};
use crate::backend::GridSize;
use crate::handle::{DEFAULT_SCROLLBACK_LINES, new_shared_terminal};
use crate::test_engine::feed;

fn model(size: GridSize, bytes: &[u8], policy: ResizePolicy) -> TerminalModel {
    let term = new_shared_terminal(size, DEFAULT_SCROLLBACK_LINES);
    feed(&mut term.lock(), bytes);
    TerminalModel::new(term, policy)
}

fn row_text(model: &TerminalModel, line: usize) -> String {
    let cells = model.query_line_range_cells(line, 1);
    cells
        .cells
        .iter()
        .map(|cell| cell.ch)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// PERF-14: `has_selection` agrees with `selection_text` without building the string.
#[test]
fn has_selection_tracks_selection_state() {
    let model = model(
        GridSize { cols: 11, lines: 1 },
        b"hello world",
        ResizePolicy::Default,
    );
    assert!(!model.has_selection());

    model.start_selection(0.0, 0.0, SelectionKind::Simple);
    model.update_selection(0.0, 4.9);
    assert!(model.has_selection());
    assert_eq!(model.selection_text().as_deref(), Some("hello"));

    model.clear_selection();
    assert!(!model.has_selection());
    assert!(model.selection_text().is_none());
}

#[test]
fn select_all_marks_a_selection() {
    let model = model(
        GridSize { cols: 3, lines: 2 },
        b"one\r\ntwo",
        ResizePolicy::Default,
    );
    model.select_all();
    assert!(model.has_selection());
    assert_eq!(model.selection_text().as_deref(), Some("one\ntwo"));
}

/// The snapshot's coordinates are the reference's: `cursor_line` counts from
/// the viewport top at `display_offset == 0`, and history is negative.
#[test]
fn scrolling_back_moves_the_display_offset_not_the_cursor_line() {
    let model = model(
        GridSize { cols: 8, lines: 2 },
        b"one\r\ntwo\r\nthree\r\nfour",
        ResizePolicy::Default,
    );
    let before = model.query_state(true);
    assert_eq!(before.display_offset, 0);
    assert_eq!(before.cursor_row, 1);
    assert_eq!(row_text(&model, 0), "three");

    model.scroll(1);
    let after = model.query_state(true);
    assert_eq!(after.display_offset, 1);
    assert_eq!(after.cursor_row, 1, "the cursor did not move");
    assert_eq!(row_text(&model, 0), "two");

    model.scroll_to_bottom();
    assert_eq!(model.query_state(true).display_offset, 0);
}

/// DEC-0008 at the adapter: the policy the backend picked reaches the engine.
/// The cell-exact contract is the engine's own
/// (`oneterm_vt::reflow::tests::keep_viewport_top_*`).
#[test]
fn resize_grid_applies_the_backend_policy() {
    let size = GridSize { cols: 10, lines: 5 };
    let mut bytes = Vec::new();
    for index in 0..20 {
        bytes.extend_from_slice(format!("line{index}\r\n").as_bytes());
    }
    bytes.extend_from_slice(b"prompt>");

    let keep = model(size, &bytes, ResizePolicy::KeepViewportTop);
    let default = model(size, &bytes, ResizePolicy::Default);
    assert!(keep.needs_resize(8, 10));
    keep.resize_grid(8, 10);
    default.resize_grid(8, 10);
    assert!(!keep.needs_resize(8, 10));

    // conhost keeps the viewport top and leaves the added rows blank.
    assert_eq!(keep.query_state(true).cursor_row, 4);
    assert_eq!(row_text(&keep, 0), "line16");
    assert_eq!(row_text(&keep, 4), "prompt>");
    assert_eq!(row_text(&keep, 7), "");

    // The reference pulls history into the top and moves the cursor down.
    assert_eq!(default.query_state(true).cursor_row, 7);
    assert_eq!(row_text(&default, 0), "line13");
    assert_eq!(row_text(&default, 7), "prompt>");
}

/// The snapshot still hands each decoded image out exactly once, and the
/// placement it leaves behind still says which cells the image covers and where
/// its top-left sits — the geometry the painter reads instead of the per-cell
/// `GraphicCell { id, col, row }` the reference stored (R-21).
#[test]
fn a_sixel_reaches_the_snapshot_once_with_its_placement() {
    // Two columns wide and one band tall: `#0;2;100;0;0` makes register 0 red,
    // `~~` fills both columns, so the image is 2x6 pixels = one 10x20 cell.
    let model = model(
        GridSize { cols: 10, lines: 4 },
        b"\x1bPq#0;2;100;0;0~~\x1b\\",
        ResizePolicy::Default,
    );

    let first = model.snapshot();
    assert_eq!(first.graphics().len(), 1, "the image is handed out once");
    let image = &first.graphics()[0];
    assert_eq!((image.width, image.height), (2, 6));

    let placements = first.placements();
    assert_eq!(placements.len(), 1, "a 2x6 image is one placement");
    let placement = &placements[0];
    assert_eq!(placement.id, image.id);
    assert_eq!(
        (placement.rows, placement.cols),
        (1, 1),
        "a 2x6 image covers one virtual cell"
    );
    assert_eq!(
        first.display_row(placement.row),
        Some(0),
        "placed at the cursor"
    );
    assert_eq!(placement.col, 0);
    // The covered cell names the image, and its offset inside the image's own
    // cell grid is derived from the placement.
    let cell = first.rows()[0].cells[0];
    assert_eq!(cell.graphic, Some(image.id));
    assert_eq!(
        first.graphic_offset(image.id, placement.row, 0),
        Some((0, 0)),
        "top-left of the image"
    );

    // The drain is the adapter's, once per snapshot: a second snapshot still
    // sees the placement, but not the pixels again.
    let second = model.snapshot();
    assert!(second.graphics().is_empty(), "handed out exactly once");
    assert_eq!(second.placements().len(), 1);
}

#[test]
fn search_reports_matches_on_their_rows() {
    let model = model(
        GridSize { cols: 8, lines: 2 },
        b"alpha\r\nbeta\r\nalpha",
        ResizePolicy::Default,
    );
    let info = model.terminal_info(0, 0);
    let matches = model.search("alpha", crate::SearchOptions::default());
    assert_eq!(matches.len(), 2);
    // The first match scrolled into history; the second is on the last row.
    assert_eq!(matches[0].grid_line(info.screen_top), -1);
    assert_eq!(matches[1].grid_line(info.screen_top), 1);
    assert_eq!(matches[1].row, info.screen_top + 1);
    assert_eq!(matches[1].start_col, 0);
    assert_eq!(matches[1].end_col, 5);
}
