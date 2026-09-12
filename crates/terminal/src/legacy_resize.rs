//! The ConPTY resize contract, kept running against the **old** engine.
//!
//! R-44 (`migration.md` § "Tests that change"): these ten tests are the only
//! written form of what conhost does behind ConPTY, so they keep running
//! against `alacritty_terminal` until `US-0082` — by which time the engine's own
//! `reflow::tests::keep_viewport_top_*` (`US-0077`) have been green in the same
//! commit. Only then is this file deleted.
//!
//! Nothing in the product path reaches this module: `TerminalModel::resize_grid`
//! calls `Terminal::resize(size, policy)` and the engine owns both policies
//! natively. The two functions below are the reference implementation the
//! assertions are written against, moved here verbatim from `model.rs` so the
//! check stays independent of the engine it is checking.

use std::mem;

use alacritty_terminal::event::{EventListener, VoidListener};
use alacritty_terminal::grid::{Dimensions, Grid, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use super::ResizePolicy;

/// Simple grid dimensions for `Term::resize`.
struct TerminalSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Resize the primary grid the way conhost resizes its buffer behind ConPTY
/// ([`ResizePolicy::KeepViewportTop`]).
///
/// Measured on Windows 11 (BUG-0051, raw PTY dumps): conhost repaints nothing
/// after `ResizePseudoConsole`. It re-wraps the rows of the old viewport at the
/// new width as if the top row started a line, keeps that content at the top,
/// leaves the rows below blank and addresses later output with absolute cursor
/// positions in those coordinates. alacritty anchors the bottom row instead:
/// `grow_lines` pulls history into the top, `grow_columns` lets history fill the
/// rows freed by joining wrapped rows, `shrink_columns` pushes the top rows out
/// when it splits rows. So this runs alacritty's resize, measures the row
/// conhost puts the cursor on ([`conhost_cursor_row`]) and shifts the viewport
/// by the difference.
fn resize_keeping_viewport_top<EP: EventListener>(term: &mut Term<EP>, lines: usize, cols: usize) {
    let parked_alt = term.mode().contains(TermMode::ALT_SCREEN).then(|| {
        let placeholder = Grid::new(term.screen_lines(), term.columns(), 0);
        let alt = mem::replace(term.grid_mut(), placeholder);
        term.swap_alt();
        alt
    });

    let display_offset = term.grid().display_offset();
    let conhost_row = conhost_cursor_row(term.grid(), cols).min(lines - 1) as i32;
    term.resize(TerminalSize { cols, lines });

    let grid = term.grid_mut();
    let shift = grid.cursor.point.line.0 - conhost_row;
    if shift > 0 {
        let shift = shift as usize;
        grid.scroll_up(&(Line(0)..Line(lines as i32)), shift);
        grid.cursor.point.line -= shift;
        grid.saved_cursor.point.line = Line((grid.saved_cursor.point.line.0 - shift as i32).max(0));
    } else if shift < 0 {
        let pull = (shift.unsigned_abs() as usize).min(grid.history_size());
        grid.resize(true, lines + pull, cols);
        grid.resize(true, lines, cols);
    }
    if shift != 0 {
        let offset_delta = display_offset as i32 - grid.display_offset() as i32;
        grid.scroll_display(Scroll::Delta(offset_delta));
        term.selection = None;
    }

    if let Some(mut alt) = parked_alt {
        alt.resize(false, lines, cols);
        term.swap_alt();
        *term.grid_mut() = alt;
    }
}

/// The viewport row conhost puts the cursor on after re-wrapping the viewport
/// to `cols` columns (measured behaviour, see [`resize_keeping_viewport_top`]).
fn conhost_cursor_row(grid: &Grid<Cell>, cols: usize) -> usize {
    let old_cols = grid.columns();
    let cursor = &grid.cursor;
    let pad = old_cols / cols + 1;
    let rows = pad + cursor.point.line.0 as usize + 1;
    let mut probe: Grid<Cell> = Grid::new(rows, old_cols, rows * old_cols);
    for line in 0..=cursor.point.line.0 {
        probe[Line(pad as i32 + line)] = grid[Line(line)].clone();
    }
    probe.cursor.point = Point::new(Line(rows as i32 - 1), cursor.point.column);
    probe.cursor.input_needs_wrap = cursor.input_needs_wrap;
    probe.resize(true, rows, cols);
    probe.history_size() + probe.cursor.point.line.0 as usize - pad
}

/// The two policies as `TerminalModel::resize_grid` applied them.
fn resize_grid(term: &mut Term<VoidListener>, policy: ResizePolicy, rows: u16, cols: u16) {
    let (lines, cols) = (rows as usize, cols as usize);
    if policy == ResizePolicy::KeepViewportTop {
        resize_keeping_viewport_top(term, lines, cols);
    } else {
        term.resize(TerminalSize { cols, lines });
    }
}

/// A 5x10 terminal whose primary screen was fed `lines` numbered lines and
/// a prompt, so the cursor sits on the last row and the oldest lines are in
/// scrollback. `bytes` are fed last.
fn fed_term(lines: usize, bytes: &[u8]) -> Term<VoidListener> {
    let config = Config {
        scrolling_history: 100,
        ..Default::default()
    };
    let mut term = Term::new(config, &TerminalSize { cols: 10, lines: 5 }, VoidListener);
    let mut parser = Processor::<StdSyncHandler>::new();
    for i in 0..lines {
        parser.advance(&mut term, format!("line{i}\r\n").as_bytes());
    }
    parser.advance(&mut term, b"prompt>");
    parser.advance(&mut term, bytes);
    term
}

/// A 5x10 terminal fed `rows` (each followed by CR LF) and then `prompt`.
fn term_with(rows: &[&str], prompt: &str) -> Term<VoidListener> {
    let config = Config {
        scrolling_history: 100,
        ..Default::default()
    };
    let mut term = Term::new(config, &TerminalSize { cols: 10, lines: 5 }, VoidListener);
    let mut parser = Processor::<StdSyncHandler>::new();
    for row in rows {
        parser.advance(&mut term, format!("{row}\r\n").as_bytes());
    }
    parser.advance(&mut term, prompt.as_bytes());
    term
}

fn wraps(term: &Term<VoidListener>, line: i32) -> bool {
    term.grid()[Line(line)][Column(term.columns() - 1)]
        .flags
        .contains(Flags::WRAPLINE)
}

fn row_text(term: &Term<VoidListener>, line: i32) -> String {
    let row = &term.grid()[Line(line)];
    (0..term.columns())
        .map(|col| row[Column(col)].c)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
    Processor::<StdSyncHandler>::new().advance(term, bytes);
}

#[test]
fn keep_viewport_top_grow_keeps_rows_cursor_and_history() {
    let mut term = fed_term(20, b"");
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 10);

    assert_eq!(term.grid().cursor.point.line, Line(4));
    assert_eq!(row_text(&term, 0), "line16");
    assert_eq!(row_text(&term, 4), "prompt>");
    for line in 5..8 {
        assert_eq!(row_text(&term, line), "", "row {line} must be blank");
    }
    assert_eq!(row_text(&term, -1), "line15");
    assert_eq!(term.history_size(), 16);
    assert_eq!(term.grid().display_offset(), 0);
}

#[test]
fn default_grow_pulls_history_and_moves_the_cursor_down() {
    let mut term = fed_term(20, b"");
    resize_grid(&mut term, ResizePolicy::Default, 8, 10);

    assert_eq!(term.grid().cursor.point.line, Line(7));
    assert_eq!(row_text(&term, 0), "line13");
    assert_eq!(row_text(&term, 7), "prompt>");
    assert_eq!(term.history_size(), 13);
}

#[test]
fn keep_viewport_top_grow_larger_than_history() {
    // Two history lines, five rows added: only two could have been pulled.
    let mut term = fed_term(6, b"");
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 10, 10);

    assert_eq!(term.grid().cursor.point.line, Line(4));
    assert_eq!(row_text(&term, 0), "line2");
    assert_eq!(row_text(&term, -2), "line0");
    for line in 5..10 {
        assert_eq!(row_text(&term, line), "", "row {line} must be blank");
    }
    assert_eq!(term.history_size(), 2);
}

#[test]
fn keep_viewport_top_repeated_grows_and_column_change() {
    let mut term = fed_term(20, b"");
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 12);
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 12, 20);

    assert_eq!(term.columns(), 20);
    assert_eq!(term.grid().cursor.point.line, Line(4));
    assert_eq!(row_text(&term, 0), "line16");
    assert_eq!(row_text(&term, 4), "prompt>");
    assert_eq!(row_text(&term, 11), "");
    assert_eq!(term.history_size(), 16);
}

#[test]
fn keep_viewport_top_restores_a_scrolled_back_viewport() {
    let mut term = fed_term(20, b"");
    term.scroll_display(Scroll::Delta(3));
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 10);

    assert_eq!(term.grid().display_offset(), 3);
    assert_eq!(term.grid().cursor.point.line, Line(4));
}

#[test]
fn keep_viewport_top_leaves_shrink_and_history_less_grow_to_alacritty() {
    for (rows, lines) in [(3, 20), (8, 2)] {
        let mut keep = fed_term(lines, b"");
        let mut default = fed_term(lines, b"");
        resize_grid(&mut keep, ResizePolicy::KeepViewportTop, rows, 10);
        resize_grid(&mut default, ResizePolicy::Default, rows, 10);

        assert_eq!(keep.grid().cursor.point, default.grid().cursor.point);
        assert_eq!(keep.history_size(), default.history_size());
        for line in -(keep.history_size() as i32)..rows as i32 {
            assert_eq!(row_text(&keep, line), row_text(&default, line));
        }
    }
}

#[test]
fn keep_viewport_top_grow_during_alt_screen_corrects_the_primary_grid() {
    let mut term = fed_term(20, b"\x1b[?1049h\x1b[Htui");
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 10);

    assert!(term.mode().contains(TermMode::ALT_SCREEN));
    assert_eq!(row_text(&term, 0), "tui");
    assert_eq!(term.grid().cursor.point, Point::new(Line(0), Column(3)));
    assert_eq!(term.history_size(), 0);

    feed(&mut term, b"\x1b[?1049l");
    assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));
    assert_eq!(row_text(&term, 0), "line16");
    assert_eq!(row_text(&term, 4), "prompt>");
    for line in 5..8 {
        assert_eq!(row_text(&term, line), "", "row {line} must be blank");
    }
    assert_eq!(term.history_size(), 16);
}

#[test]
fn default_grow_during_alt_screen_pulls_the_primary_history() {
    let mut term = fed_term(20, b"\x1b[?1049h\x1b[Htui");
    resize_grid(&mut term, ResizePolicy::Default, 8, 10);

    feed(&mut term, b"\x1b[?1049l");
    assert_eq!(term.grid().cursor.point.line, Line(7));
    assert_eq!(term.history_size(), 13);
}

/// `ls -lath` case (BUG-0051 rework): a wrapped line inside the viewport
/// joins on a widen; conhost keeps the top row and moves the cursor up by
/// the rows that vanished, so must the grid. The joined rows return to
/// history instead of being replaced by pulled history.
#[test]
fn keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row() {
    let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    rows.push("abcdefghijklmnopqrstuvwxyz");
    let mut term = term_with(&rows, "prompt>");
    assert_eq!(row_text(&term, 0), "line9");
    assert!(wraps(&term, 1) && wraps(&term, 2) && !wraps(&term, 3));
    assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));

    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 30);

    assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(7)));
    assert_eq!(row_text(&term, 0), "line9");
    assert_eq!(row_text(&term, 1), "abcdefghijklmnopqrstuvwxyz");
    assert!(!wraps(&term, 1));
    assert_eq!(row_text(&term, 2), "prompt>");
    for line in 3..8 {
        assert_eq!(row_text(&term, line), "", "row {line} must be blank");
    }
    assert_eq!(row_text(&term, -1), "line8");
    assert_eq!(term.history_size(), 9);
}

/// A pure widen (rows unchanged) needs the same correction: conhost moves
/// the cursor up by the joined rows while alacritty keeps its index.
#[test]
fn keep_viewport_top_widen_without_row_change_moves_the_cursor_up() {
    let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    rows.push("abcdefghijklmnopqrstuvwxyz");
    let mut term = term_with(&rows, "prompt>");
    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 5, 30);

    assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(7)));
    assert_eq!(row_text(&term, 0), "line9");
    assert_eq!(row_text(&term, 1), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(row_text(&term, 2), "prompt>");
    assert_eq!(row_text(&term, 3), "");
    assert_eq!(row_text(&term, 4), "");
    assert_eq!(term.history_size(), 9);
}

/// The top row is the tail of a line wrapped from history. conhost re-wraps
/// the viewport from its top row, so nothing above the cursor changes and
/// the cursor row stays; alacritty joins the tail into the history line,
/// which then shows whole on the top row.
#[test]
fn keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row() {
    let rows: Vec<String> = (0..8).map(|i| format!("line{i}")).collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    rows.extend(["abcdefghijklmnopqrstuvwxyz", "a", "b", "c"]);
    let mut term = term_with(&rows, "prompt>");
    assert_eq!(row_text(&term, 0), "uvwxyz");
    assert!(wraps(&term, -1) && !wraps(&term, 0));

    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 30);

    assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));
    assert_eq!(row_text(&term, 0), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(row_text(&term, 1), "a");
    assert_eq!(row_text(&term, 3), "c");
    assert_eq!(row_text(&term, 4), "prompt>");
    assert_eq!(row_text(&term, 5), "");
    assert_eq!(row_text(&term, -1), "line7");
    assert_eq!(term.history_size(), 8);
}

/// The cursor sits on the continuation row of a wrapped command line: the
/// join moves it up one row and right by the joined width, as in conhost.
#[test]
fn keep_viewport_top_widen_joins_the_cursor_row() {
    let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut term = term_with(&rows, "prompt>abcdef");
    assert!(wraps(&term, 3));
    assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(3)));

    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 5, 30);

    assert_eq!(term.grid().cursor.point, Point::new(Line(3), Column(13)));
    assert_eq!(row_text(&term, 0), "line7");
    assert_eq!(row_text(&term, 3), "prompt>abcdef");
    assert_eq!(row_text(&term, 4), "");
    assert_eq!(term.history_size(), 7);
}

/// A column shrink splits a row above a mid-screen cursor: conhost keeps
/// the top row and the cursor moves down; alacritty pushed the split row
/// into history, so the grid pulls it back and drops a blank bottom row.
#[test]
fn keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back() {
    let mut term = term_with(&["abcdefgh"], "p>");
    assert_eq!(term.grid().cursor.point, Point::new(Line(1), Column(2)));

    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 5, 6);

    assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(2)));
    assert_eq!(row_text(&term, 0), "abcdef");
    assert!(wraps(&term, 0));
    assert_eq!(row_text(&term, 1), "gh");
    assert_eq!(row_text(&term, 2), "p>");
    assert_eq!(row_text(&term, 3), "");
    assert_eq!(row_text(&term, 4), "");
    assert_eq!(term.history_size(), 0);
}

/// A column shrink with the cursor on the bottom row: the split row pushes
/// the top row out on both sides, so the default result already matches.
#[test]
fn keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_alacritty() {
    let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    rows.push("abcdefgh");
    let mut keep = term_with(&rows, "p>");
    let mut default = term_with(&rows, "p>");
    resize_grid(&mut keep, ResizePolicy::KeepViewportTop, 5, 6);
    resize_grid(&mut default, ResizePolicy::Default, 5, 6);

    assert_eq!(keep.grid().cursor.point, Point::new(Line(4), Column(2)));
    assert_eq!(keep.grid().cursor.point, default.grid().cursor.point);
    assert_eq!(keep.history_size(), default.history_size());
    for line in -(keep.history_size() as i32)..5 {
        assert_eq!(row_text(&keep, line), row_text(&default, line));
    }
}

/// Wrapped rows on the primary screen join while a TUI holds the alt
/// screen: the correction runs on the inactive primary grid and the prompt
/// lands on the joined row after the TUI exits.
#[test]
fn keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows() {
    let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
    let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    rows.push("abcdefghijklmnopqrstuvwxyz");
    let mut term = term_with(&rows, "prompt>tui\r\n");
    assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(0)));
    feed(&mut term, b"\x1b[?1049h\x1b[Htui");

    resize_grid(&mut term, ResizePolicy::KeepViewportTop, 8, 30);

    assert!(term.mode().contains(TermMode::ALT_SCREEN));
    feed(&mut term, b"\x1b[?1049l");
    assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(0)));
    assert_eq!(row_text(&term, 0), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(row_text(&term, 1), "prompt>tui");
    for line in 2..8 {
        assert_eq!(row_text(&term, line), "", "row {line} must be blank");
    }
    assert_eq!(term.history_size(), 10);
}
