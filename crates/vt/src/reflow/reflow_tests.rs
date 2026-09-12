//! `reflow::tests::` — the reference's own reflow cases, the ten
//! `keep_viewport_top_*` scenarios the ConPTY policy is pinned by, and one test
//! per trap the design's Verification section names.
//!
//! The `keep_viewport_top_*` scenarios are `crates/terminal/src/model.rs`'s,
//! with the same inputs and the same expected grids: they are the exit criterion
//! for replacing `resize_keeping_viewport_top`. The old suite keeps running
//! against the old engine until the adapter packet (R-44).

use super::*;
use crate::cell::{CellContent, CellWidth};
use crate::grid::{
    AnchorKind, PrintMode, RowFlags, RowId, Screen, ScrollRegion, TerminalGrid, Viewport,
};
use crate::intern::Interner;

struct Fixture {
    grid: TerminalGrid,
    interner: Interner,
}

/// The shape `model.rs`'s tests use: 5x10 with a 100-row scrollback.
fn fixture(rows: u16, cols: u16, scrollback: u32) -> Fixture {
    Fixture {
        grid: TerminalGrid::new(Size { rows, cols }, scrollback),
        interner: Interner::default(),
    }
}

/// `model.rs::fed_term`: `lines` numbered lines and a prompt on a 5x10 screen.
fn fed(lines: usize) -> Fixture {
    let mut f = fixture(5, 10, 100);
    for index in 0..lines {
        f.feed(&format!("line{index}\r\n"));
    }
    f.feed("prompt>");
    f
}

/// `model.rs::term_with`: each row followed by CR LF, then the prompt.
fn term_with(rows: &[&str], prompt: &str) -> Fixture {
    let mut f = fixture(5, 10, 100);
    for row in rows {
        f.feed(&format!("{row}\r\n"));
    }
    f.feed(prompt);
    f
}

fn numbered(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("line{index}")).collect()
}

impl Fixture {
    fn feed(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\r' => self.grid.screen_mut().carriage_return(),
                '\n' => {
                    self.grid.linefeed();
                }
                _ => self.grid.print(c, PrintMode::default(), &mut self.interner),
            }
        }
    }

    fn resize(&mut self, rows: u16, cols: u16, policy: ResizePolicy) -> ResizeOutcome {
        self.grid.resize(Size { rows, cols }, policy)
    }

    fn screen(&self) -> &Screen {
        self.grid.screen()
    }

    fn primary(&self) -> &Screen {
        self.grid.primary()
    }

    /// A screen row by index; negative indices reach into history, the way
    /// `model.rs`'s `row_text` uses `Line(-1)`.
    fn row_id(&self, index: i32) -> RowId {
        let top = self.screen().screen_top();
        if index < 0 {
            top - index.unsigned_abs() as u64
        } else {
            top + index as u64
        }
    }

    fn row_text(&self, index: i32) -> String {
        self.text_of(self.screen(), self.row_id(index))
    }

    fn primary_row_text(&self, index: i32) -> String {
        let top = self.primary().screen_top();
        let id = if index < 0 {
            top - index.unsigned_abs() as u64
        } else {
            top + index as u64
        };
        self.text_of(self.primary(), id)
    }

    fn text_of(&self, screen: &Screen, id: RowId) -> String {
        let mut out = String::new();
        screen.row_text(id, &self.interner.graphemes, &mut out);
        out.trim_end().to_owned()
    }

    fn wraps(&self, index: i32) -> bool {
        self.screen().row(self.row_id(index)).wrapped()
    }

    fn cursor(&self) -> (u16, u16) {
        let screen = self.screen();
        (screen.cursor_row_index(), screen.cursor().pos.col)
    }

    fn primary_cursor(&self) -> (u16, u16) {
        let screen = self.primary();
        (screen.cursor_row_index(), screen.cursor().pos.col)
    }

    fn history(&self) -> u32 {
        self.screen().history_len()
    }

    fn all_rows(&self) -> Vec<String> {
        let screen = self.screen();
        let mut out = Vec::new();
        let mut id = screen.oldest();
        while id <= screen.newest() {
            out.push(self.text_of(screen, id));
            id = id + 1;
        }
        out
    }

    /// Enter the alternate screen the way `? 1049 h` does, then home and print.
    fn enter_alt(&mut self, text: &str) {
        self.grid.swap_alt();
        self.grid.screen_mut().goto(0, 0);
        self.feed(text);
    }
}

// ── The reference's own reflow cases ────────────────────────────────────────

#[test]
fn shrink_joins_then_splits() {
    let mut f = fixture(1, 5, 2);
    f.feed("12345");

    f.resize(1, 2, ResizePolicy::BottomAnchor);

    assert_eq!(f.all_rows(), ["12", "34", "5"]);
    assert!(f.wraps(-2) && f.wraps(-1) && !f.wraps(0));
    assert_eq!(f.history(), 2);
}

#[test]
fn shrink_twice_is_stable() {
    let mut f = fixture(1, 5, 2);
    f.feed("12345");

    f.resize(1, 4, ResizePolicy::BottomAnchor);
    f.resize(1, 2, ResizePolicy::BottomAnchor);

    assert_eq!(f.all_rows(), ["12", "34", "5"]);
    assert_eq!(f.history(), 2);
}

#[test]
fn shrink_with_an_empty_cell_inside_the_line() {
    let mut f = fixture(1, 5, 3);
    f.feed("1 34");

    f.resize(1, 2, ResizePolicy::BottomAnchor);
    assert_eq!(f.all_rows(), ["1", "34"]);
    assert!(f.wraps(-1), "the interior blank keeps the line joined");
    assert_eq!(f.history(), 1);

    f.resize(1, 1, ResizePolicy::BottomAnchor);
    assert_eq!(f.all_rows(), ["1", "", "3", "4"]);
    assert_eq!(f.history(), 3);
}

#[test]
fn grow_rejoins_wrapped_rows() {
    let mut f = fixture(2, 2, 0);
    f.feed("123");
    assert!(f.wraps(0));

    f.resize(2, 3, ResizePolicy::BottomAnchor);

    assert_eq!(f.all_rows(), ["123", ""]);
    assert!(!f.wraps(0));
    assert_eq!(f.history(), 0);
}

#[test]
fn grow_rejoins_across_multiple_rows() {
    let mut f = fixture(3, 2, 0);
    f.feed("123456");

    f.resize(3, 6, ResizePolicy::BottomAnchor);

    assert_eq!(f.all_rows(), ["123456", "", ""]);
    assert_eq!(f.history(), 0);
}

// ── Traps ───────────────────────────────────────────────────────────────────

#[test]
fn alt_screen_grow_and_shrink_do_not_reflow() {
    let mut f = fixture(3, 10, 100);
    f.enter_alt("abcdefghijklm");
    assert!(f.wraps(0), "the alternate screen wraps like any other");

    f.resize(3, 20, ResizePolicy::BottomAnchor);
    assert_eq!(
        f.all_rows(),
        ["abcdefghij", "klm", ""],
        "trap 29: padded, never rejoined"
    );
    assert!(f.wraps(0), "and the wrap flag is left exactly where it was");

    f.resize(3, 6, ResizePolicy::BottomAnchor);
    assert_eq!(
        f.all_rows(),
        ["abcdef", "klm", ""],
        "trap 29: truncated, never split"
    );
    assert_eq!(f.screen().cursor().pos.col, 3);
}

#[test]
fn rows_only_grow_moves_the_cursor_down_by_the_pulled_rows() {
    let mut f = fed(20);
    assert_eq!(f.cursor(), (4, 7));

    let outcome = f.resize(8, 10, ResizePolicy::BottomAnchor);

    assert!(!outcome.reflowed, "the column count did not change");
    assert_eq!(f.cursor(), (7, 7));
    assert_eq!(f.row_text(0), "line13");
    assert_eq!(f.history(), 13);
}

#[test]
fn rows_only_shrink_pushes_rows_into_history() {
    let mut f = fed(20);

    f.resize(3, 10, ResizePolicy::BottomAnchor);

    assert_eq!(f.cursor(), (2, 7), "the cursor stays on the bottom row");
    assert_eq!(f.row_text(2), "prompt>");
    assert_eq!(f.history(), 18);
}

#[test]
fn resize_resets_the_scroll_region_and_clears_the_selection() {
    let mut f = fed(20);
    f.grid
        .screen_mut()
        .set_region(1, 4, crate::grid::CursorOrigin::Screen);
    let pos = Pos {
        row: f.screen().screen_top(),
        col: 0,
    };
    let start = f
        .grid
        .anchors_mut()
        .register(AnchorKind::SelectionStart, pos);
    let end = f.grid.anchors_mut().register(AnchorKind::SelectionEnd, pos);

    f.resize(6, 10, ResizePolicy::BottomAnchor);
    assert_eq!(
        f.screen().region(),
        ScrollRegion::full(6),
        "trap 28: any resize destroys the region"
    );
    assert!(
        f.grid.anchors().get(start).is_some(),
        "a rows-only resize keeps the selection"
    );

    f.resize(6, 14, ResizePolicy::BottomAnchor);
    assert!(f.grid.anchors().get(start).is_none());
    assert!(f.grid.anchors().get(end).is_none());
}

#[test]
fn first_visible_character_survives_a_resize() {
    let mut f = fed(20);
    f.grid.screen_mut().scroll_viewport(-4);
    let before = f.screen().viewport().top;
    let text = f.text_of(f.screen(), before);
    assert_eq!(text, "line12");

    f.resize(5, 30, ResizePolicy::BottomAnchor);

    let Viewport { top, .. } = f.screen().viewport();
    assert_eq!(
        f.text_of(f.screen(), top),
        text,
        "trap 30, R-06: the first visible character is still the first visible one"
    );
}

#[test]
fn sticky_bottom_survives_a_resize() {
    let mut f = fed(20);
    assert_eq!(f.screen().scroll_offset(), 0);

    f.resize(7, 30, ResizePolicy::BottomAnchor);

    assert_eq!(f.screen().scroll_offset(), 0);
    assert_eq!(f.row_text(f.cursor().0 as i32), "prompt>");
}

#[test]
fn pending_wrap_is_rearmed_only_without_the_wrapped_flag() {
    // The cursor fills the last column of a row nothing continues.
    let mut f = fixture(2, 6, 10);
    f.feed("abcdef");
    assert!(f.screen().cursor().pending_wrap);

    f.resize(2, 3, ResizePolicy::BottomAnchor);
    assert!(
        f.screen().cursor().pending_wrap,
        "trap 31: still past the last column of a row that does not continue"
    );
    // The split pushed "abc" into history, so the cursor's own row is the top
    // visible one — the reference lands in the same place.
    assert_eq!(f.cursor(), (0, 2));
    assert_eq!(f.row_text(0), "def");
    assert_eq!(f.history(), 1);

    // Widening leaves the cursor inside the row, so the wrap is not re-armed.
    f.resize(2, 10, ResizePolicy::BottomAnchor);
    assert!(!f.screen().cursor().pending_wrap);
    assert_eq!(f.cursor(), (0, 6));
}

#[test]
fn shrink_columns_truncates_the_oldest_history() {
    let mut f = fixture(2, 8, 4);
    for index in 0..5 {
        f.feed(&format!(
            "{index}{index}{index}{index}{index}{index}{index}{index}"
        ));
        f.feed("\r\n");
    }
    assert_eq!(f.history(), 4, "the limit is already reached");

    let outcome = f.resize(2, 4, ResizePolicy::BottomAnchor);

    assert!(outcome.reflowed);
    assert!(
        outcome.rows_trimmed > 0,
        "trap 32: splitting rows pushes the oldest out"
    );
    assert_eq!(f.history(), 4, "history never exceeds its limit");
    assert_eq!(f.screen().oldest(), f.screen().newest() - 5);
}

#[test]
fn wide_char_at_the_new_last_column_moves_to_the_next_row() {
    let mut f = fixture(3, 6, 10);
    f.feed("ab\u{ff21}cd");
    assert_eq!(f.row_text(0), "ab\u{ff21}cd");

    f.resize(3, 3, ResizePolicy::BottomAnchor);

    // "ab" fills two of three columns, so the wide glyph cannot start in the
    // last one: a leading spacer holds the place and the glyph wraps. The split
    // pushed both rows into history, so they are addressed from `oldest`.
    let split = f.screen().oldest();
    assert_eq!(
        f.screen().row(split).cell(2).width(),
        CellWidth::LeadingWideSpacer
    );
    assert!(f.screen().row(split).wrapped());
    assert_eq!(f.screen().row(split + 1).cell(0).width(), CellWidth::Wide);
    assert_eq!(
        f.screen().row(split + 1).cell(0).content(),
        CellContent::Scalar('\u{ff21}')
    );
    assert_eq!(
        f.screen().row(split + 1).cell(1).width(),
        CellWidth::WideSpacer
    );
    assert_eq!(f.text_of(f.screen(), split + 2), "d");

    // Widening again drops the spacer and rejoins the line unchanged.
    f.resize(3, 6, ResizePolicy::BottomAnchor);
    assert_eq!(f.row_text(0), "ab\u{ff21}cd");
    assert!(!f.wraps(0));
}

// ── The ConPTY policy: the ten `keep_viewport_top_*` scenarios ───────────────

#[test]
fn keep_viewport_top_grow_keeps_rows_cursor_and_history() {
    let mut f = fed(20);

    f.resize(8, 10, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor().0, 4);
    assert_eq!(f.row_text(0), "line16");
    assert_eq!(f.row_text(4), "prompt>");
    for index in 5..8 {
        assert_eq!(f.row_text(index), "", "row {index} must be blank");
    }
    assert_eq!(f.row_text(-1), "line15");
    assert_eq!(f.history(), 16);
    assert_eq!(f.screen().scroll_offset(), 0);
}

#[test]
fn default_policy_grow_anchors_the_bottom_row() {
    let mut f = fed(20);

    f.resize(8, 10, ResizePolicy::BottomAnchor);

    assert_eq!(f.cursor().0, 7);
    assert_eq!(f.row_text(0), "line13");
    assert_eq!(f.row_text(7), "prompt>");
    assert_eq!(f.history(), 13);
}

#[test]
fn keep_viewport_top_grow_larger_than_history() {
    // Two history lines, five rows added: only two could have been pulled.
    let mut f = fed(6);

    f.resize(10, 10, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor().0, 4);
    assert_eq!(f.row_text(0), "line2");
    assert_eq!(f.row_text(-2), "line0");
    for index in 5..10 {
        assert_eq!(f.row_text(index), "", "row {index} must be blank");
    }
    assert_eq!(f.history(), 2);
}

#[test]
fn keep_viewport_top_repeated_grows_and_column_change() {
    let mut f = fed(20);

    f.resize(8, 12, ResizePolicy::KeepViewportTop);
    f.resize(12, 20, ResizePolicy::KeepViewportTop);

    assert_eq!(f.screen().cols(), 20);
    assert_eq!(f.cursor().0, 4);
    assert_eq!(f.row_text(0), "line16");
    assert_eq!(f.row_text(4), "prompt>");
    assert_eq!(f.row_text(11), "");
    assert_eq!(f.history(), 16);
}

#[test]
fn keep_viewport_top_restores_the_scroll_offset() {
    let mut f = fed(20);
    f.grid.screen_mut().scroll_viewport(-3);
    assert_eq!(f.screen().scroll_offset(), 3);

    f.resize(8, 10, ResizePolicy::KeepViewportTop);

    assert_eq!(f.screen().scroll_offset(), 3);
    assert_eq!(f.cursor().0, 4);
}

#[test]
fn keep_viewport_top_leaves_shrink_and_history_less_grow_to_bottom_anchor() {
    for (rows, lines) in [(3u16, 20usize), (8, 2)] {
        let mut keep = fed(lines);
        let mut default = fed(lines);
        keep.resize(rows, 10, ResizePolicy::KeepViewportTop);
        default.resize(rows, 10, ResizePolicy::BottomAnchor);

        assert_eq!(
            keep.cursor(),
            default.cursor(),
            "{rows} rows, {lines} lines"
        );
        assert_eq!(keep.history(), default.history());
        assert_eq!(keep.all_rows(), default.all_rows());
    }
}

#[test]
fn keep_viewport_top_corrects_the_primary_screen_while_alt_is_active() {
    let mut f = fed(20);
    f.enter_alt("tui");

    f.resize(8, 10, ResizePolicy::KeepViewportTop);

    assert!(f.grid.alt_active());
    assert_eq!(f.row_text(0), "tui", "the alternate screen is left alone");
    assert_eq!(f.cursor(), (0, 3));
    assert_eq!(f.history(), 0);

    // `measure_rows` read the primary cursor, so the primary is corrected.
    assert_eq!(f.primary_cursor(), (4, 7));
    assert_eq!(f.primary_row_text(0), "line16");
    assert_eq!(f.primary_row_text(4), "prompt>");
    for index in 5..8 {
        assert_eq!(f.primary_row_text(index), "", "row {index} must be blank");
    }
    assert_eq!(f.primary().history_len(), 16);

    f.grid.swap_alt();
    assert_eq!(f.cursor(), (4, 7));
}

#[test]
fn default_grow_during_alt_screen_pulls_the_primary_history() {
    let mut f = fed(20);
    f.enter_alt("tui");

    f.resize(8, 10, ResizePolicy::BottomAnchor);

    f.grid.swap_alt();
    assert_eq!(f.cursor().0, 7);
    assert_eq!(f.history(), 13);
}

/// The `ls -lath` case (BUG-0051 rework): a wrapped line inside the viewport
/// joins on a widen; conhost keeps the top row and moves the cursor up by the
/// rows that vanished, so must the grid.
#[test]
fn keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row() {
    let mut rows = numbered(10);
    rows.push("abcdefghijklmnopqrstuvwxyz".to_owned());
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut f = term_with(&rows, "prompt>");
    assert_eq!(f.row_text(0), "line9");
    assert!(f.wraps(1) && f.wraps(2) && !f.wraps(3));
    assert_eq!(f.cursor(), (4, 7));

    f.resize(8, 30, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor(), (2, 7));
    assert_eq!(f.row_text(0), "line9");
    assert_eq!(f.row_text(1), "abcdefghijklmnopqrstuvwxyz");
    assert!(!f.wraps(1));
    assert_eq!(f.row_text(2), "prompt>");
    for index in 3..8 {
        assert_eq!(f.row_text(index), "", "row {index} must be blank");
    }
    assert_eq!(f.row_text(-1), "line8");
    assert_eq!(f.history(), 9);
}

/// A pure widen (rows unchanged) needs the same correction.
#[test]
fn keep_viewport_top_widen_without_row_change_moves_the_cursor_up() {
    let mut rows = numbered(10);
    rows.push("abcdefghijklmnopqrstuvwxyz".to_owned());
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut f = term_with(&rows, "prompt>");

    f.resize(5, 30, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor(), (2, 7));
    assert_eq!(f.row_text(0), "line9");
    assert_eq!(f.row_text(1), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(f.row_text(2), "prompt>");
    assert_eq!(f.row_text(3), "");
    assert_eq!(f.row_text(4), "");
    assert_eq!(f.history(), 9);
}

/// The top row is the tail of a line wrapped from history. conhost re-wraps the
/// viewport from its top row, so the cursor row stays.
#[test]
fn keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row() {
    let mut rows = numbered(8);
    rows.extend(
        ["abcdefghijklmnopqrstuvwxyz", "a", "b", "c"]
            .iter()
            .map(|row| (*row).to_owned()),
    );
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut f = term_with(&rows, "prompt>");
    assert_eq!(f.row_text(0), "uvwxyz");
    assert!(f.wraps(-1) && !f.wraps(0));

    f.resize(8, 30, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor(), (4, 7));
    assert_eq!(f.row_text(0), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(f.row_text(1), "a");
    assert_eq!(f.row_text(3), "c");
    assert_eq!(f.row_text(4), "prompt>");
    assert_eq!(f.row_text(5), "");
    assert_eq!(f.row_text(-1), "line7");
    assert_eq!(f.history(), 8);
}

/// The cursor sits on the continuation row of a wrapped command line: the join
/// moves it up one row and right by the joined width, as in conhost.
#[test]
fn keep_viewport_top_widen_joins_the_cursor_row() {
    let rows = numbered(10);
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut f = term_with(&rows, "prompt>abcdef");
    assert!(f.wraps(3));
    assert_eq!(f.cursor(), (4, 3));

    f.resize(5, 30, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor(), (3, 13));
    assert_eq!(f.row_text(0), "line7");
    assert_eq!(f.row_text(3), "prompt>abcdef");
    assert_eq!(f.row_text(4), "");
    assert_eq!(f.history(), 7);
}

/// A column shrink splits a row above a mid-screen cursor: the grid pulls the
/// split rows back out of history and drops a blank bottom row.
#[test]
fn keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back() {
    let mut f = term_with(&["abcdefgh"], "p>");
    assert_eq!(f.cursor(), (1, 2));

    f.resize(5, 6, ResizePolicy::KeepViewportTop);

    assert_eq!(f.cursor(), (2, 2));
    assert_eq!(f.row_text(0), "abcdef");
    assert!(f.wraps(0));
    assert_eq!(f.row_text(1), "gh");
    assert_eq!(f.row_text(2), "p>");
    assert_eq!(f.row_text(3), "");
    assert_eq!(f.row_text(4), "");
    assert_eq!(f.history(), 0);
}

/// A column shrink with the cursor on the bottom row: the split row pushes the
/// top row out on both sides, so the default result already matches.
#[test]
fn keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_bottom_anchor() {
    let mut rows = numbered(10);
    rows.push("abcdefgh".to_owned());
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut keep = term_with(&rows, "p>");
    let mut default = term_with(&rows, "p>");

    keep.resize(5, 6, ResizePolicy::KeepViewportTop);
    default.resize(5, 6, ResizePolicy::BottomAnchor);

    assert_eq!(keep.cursor(), (4, 2));
    assert_eq!(keep.cursor(), default.cursor());
    assert_eq!(keep.history(), default.history());
    assert_eq!(keep.all_rows(), default.all_rows());
}

/// Wrapped rows on the primary screen join while a TUI holds the alternate one.
#[test]
fn keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows() {
    let mut rows = numbered(10);
    rows.push("abcdefghijklmnopqrstuvwxyz".to_owned());
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut f = term_with(&rows, "prompt>tui\r\n");
    assert_eq!(f.cursor(), (4, 0));
    f.enter_alt("tui");

    f.resize(8, 30, ResizePolicy::KeepViewportTop);

    assert!(f.grid.alt_active());
    f.grid.swap_alt();
    assert_eq!(f.cursor(), (2, 0));
    assert_eq!(f.row_text(0), "abcdefghijklmnopqrstuvwxyz");
    assert_eq!(f.row_text(1), "prompt>tui");
    for index in 2..8 {
        assert_eq!(f.row_text(index), "", "row {index} must be blank");
    }
    assert_eq!(f.history(), 10);
}

#[test]
fn keep_viewport_top_moves_the_saved_cursor() {
    let mut f = fed(20);
    f.grid.screen_mut().save_cursor();
    let saved_before = f.screen().saved_cursor().pos;
    assert_eq!(f.screen().index_of(saved_before.row), Some(4));

    f.resize(8, 10, ResizePolicy::KeepViewportTop);

    // Step 4: the saved cursor is a tracked anchor, so the shift carries it.
    let saved = f.screen().saved_cursor().pos;
    assert_eq!(f.screen().index_of(saved.row), Some(4));
    assert_eq!(saved.col, 7);
    assert_eq!(f.text_of(f.screen(), saved.row), "prompt>");
}

// ── `measure_rows` against the recorded conhost rows ─────────────────────────

/// A 33-row viewport whose rows above the cursor form `lines` logical lines of
/// the given lengths, with the cursor alone on the bottom row.
fn measured_viewport(cols: u16, lengths: &[usize]) -> Fixture {
    let mut f = fixture(33, cols, 1_000);
    for length in lengths {
        f.feed(&"x".repeat(*length));
        f.feed("\r\n");
    }
    f.feed("p");
    f
}

/// The three rows measured from conhost behind ConPTY in BUG-0051
/// § Measurements (b), reproduced from the geometry that document records.
///
/// **Host version (R-39).** The numbers are a property of a specific conhost
/// build: measured on Windows 11 on 2026-09-09 against the then-bundled ConPTY
/// pair. `crates/app/assets/conpty-manifest.json` records the pair the tree
/// carries today (`1.24.260710001`, file version `1.24.2607.10001`); BUG-0051
/// does not record the version it measured, which is the gap
/// `IN-0030`'s bump checklist has to close by re-capturing these.
#[test]
fn measure_rows_matches_the_recorded_conhost_rows() {
    // `ls -lath` at 33x43: eleven logical lines over the 32 rows above the
    // cursor, two of them still wrapping at 132 columns.
    let mut lengths = vec![140, 140];
    lengths.extend([129; 6]);
    lengths.extend([86; 3]);
    let f = measured_viewport(43, &lengths);
    assert_eq!(f.cursor().0, 32, "the cursor is on the bottom row");

    assert_eq!(
        measure_rows(f.primary(), 158),
        11,
        "maximize 33x43 to 52x158: ESC[12;18H, 21 wrapped rows joined"
    );
    assert_eq!(
        measure_rows(f.primary(), 132),
        13,
        "widen 33x43 to 33x132: ESC[14;18H, 19 joined, two lines still wrap"
    );

    // `ls` at 33x43: 32 single rows above the cursor, five of them long enough
    // to split at 34 columns.
    let mut lengths = vec![40; 5];
    lengths.extend([30; 27]);
    let f = measured_viewport(43, &lengths);
    assert_eq!(f.cursor().0, 32);
    assert_eq!(
        measure_rows(f.primary(), 34),
        37,
        "rows grow and columns shrink 33x43 to 49x34: ESC[38;18H, 5 split rows"
    );
}

#[test]
fn measure_rows_never_looks_above_the_screen() {
    // History above the viewport never enters the count, even when the top row
    // continues a line that started in it.
    let mut rows = numbered(8);
    rows.extend(
        ["abcdefghijklmnopqrstuvwxyz", "a", "b", "c"]
            .iter()
            .map(|row| (*row).to_owned()),
    );
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let f = term_with(&rows, "prompt>");
    assert_eq!(f.row_text(0), "uvwxyz");
    assert!(f.wraps(-1), "the top row continues a line from history");

    assert_eq!(
        measure_rows(f.primary(), 30),
        4,
        "the top row starts a logical line, so nothing above it joins"
    );
}

// ── Shape and bounds ────────────────────────────────────────────────────────

#[test]
fn an_unchanged_size_is_an_identity() {
    let mut f = fed(20);
    let before = f.all_rows();
    let newest = f.screen().newest();

    let outcome = f.resize(5, 10, ResizePolicy::KeepViewportTop);

    assert_eq!(outcome, ResizeOutcome::default());
    assert_eq!(f.all_rows(), before);
    assert_eq!(f.screen().newest(), newest, "not one row id was allocated");
}

#[test]
fn a_zero_or_oversized_resize_is_clamped() {
    let mut f = fed(20);

    f.resize(0, 0, ResizePolicy::BottomAnchor);
    assert_eq!(f.screen().size(), Size { rows: 1, cols: 1 });

    f.resize(4_000, 9_000, ResizePolicy::BottomAnchor);
    assert_eq!(
        f.screen().size(),
        Size {
            rows: crate::grid::MAX_ROWS,
            cols: crate::grid::MAX_COLS
        }
    );
}

#[test]
fn an_anchor_on_trimmed_whitespace_lands_at_the_end_of_its_line() {
    let mut f = fixture(2, 10, 10);
    f.feed("abc");
    let pos = Pos {
        row: f.screen().screen_top(),
        col: 8,
    };
    let mark = f.grid.anchors_mut().register(AnchorKind::Mark(1), pos);

    f.resize(2, 20, ResizePolicy::BottomAnchor);

    let moved = f.grid.anchors().get(mark).expect("the anchor survives");
    assert_eq!(moved.col, 3, "the end of the logical line, not column 8");
}

/// Found by `reflow::props::integrity_holds_after_any_resize_sequence`: a row
/// shrink pushes `(cursor_row_index + 1) - rows` rows into history, and the
/// alternate screen has none to push them into.
#[test]
fn a_rows_shrink_never_leaves_the_alt_screen_with_history() {
    let mut f = fixture(6, 10, 100);
    f.enter_alt("a\r\nb\r\nc");
    f.grid.screen_mut().goto(0, 0);

    f.resize(2, 10, ResizePolicy::BottomAnchor);

    assert_eq!(f.screen().history_len(), 0);
    assert_eq!(f.screen().newest().distance(f.screen().oldest()) + 1, 2);
}

/// Found by the same property: widening a row without reflowing left the
/// `LeadingWideSpacer` that stood in the old last column stranded inland, where
/// it means nothing.
#[test]
fn a_leading_wide_spacer_never_survives_inland() {
    let mut f = fixture(2, 4, 10);
    f.enter_alt("abc\u{ff21}");
    assert_eq!(
        f.screen().row(f.row_id(0)).cell(3).width(),
        CellWidth::LeadingWideSpacer
    );

    f.resize(2, 8, ResizePolicy::BottomAnchor);

    assert_eq!(
        f.screen().row(f.row_id(0)).cell(3).width(),
        CellWidth::Narrow
    );
}

#[test]
fn a_graphic_hint_survives_a_reflow() {
    let mut f = fixture(2, 10, 10);
    f.feed("abcdefghijkl");
    let id = f.screen().screen_top();
    f.grid.screen_mut().row_mut(id).mark_graphic();

    f.resize(2, 20, ResizePolicy::BottomAnchor);

    let joined = f.screen().screen_top();
    assert!(
        f.screen()
            .row(joined)
            .flags()
            .contains(RowFlags::HAS_GRAPHIC)
    );
}

// ── Cost (R-29): recorded, never gated ──────────────────────────────────────

/// `cargo test -p oneterm-vt --release resize_latency -- --ignored --nocapture`
///
/// Tier 4 of `testing-and-bench.md`: 80x24 to 100x40 at three scrollback
/// depths, the same geometry `vt-bench resize` uses for the engine being
/// replaced. Ignored because it is a measurement, not an assertion (R-29).
#[test]
#[ignore = "measurement, run in release"]
fn resize_latency() {
    use std::time::Instant;

    const RUNS: usize = 20;
    for depth in [0u32, 10_000, 100_000] {
        let mut f = fixture(24, 80, depth);
        // Content that actually has to reflow: 96-character lines wrap at 80.
        let lines = (depth as usize + 24).div_ceil(2).max(1);
        for index in 0..lines {
            f.feed(&format!("{:0>96}\r\n", index % 1000));
        }
        let (mut grow, mut shrink) = (Vec::new(), Vec::new());
        for _ in 0..RUNS {
            let start = Instant::now();
            f.resize(40, 100, ResizePolicy::BottomAnchor);
            grow.push(start.elapsed());
            let start = Instant::now();
            f.resize(24, 80, ResizePolicy::BottomAnchor);
            shrink.push(start.elapsed());
        }
        // The same height change without a width change, so the reflow's share
        // of the cost above is the difference.
        let mut rows_only = Vec::new();
        for _ in 0..RUNS {
            let start = Instant::now();
            f.resize(40, 80, ResizePolicy::BottomAnchor);
            f.resize(24, 80, ResizePolicy::BottomAnchor);
            rows_only.push(start.elapsed());
        }
        grow.sort_unstable();
        shrink.sort_unstable();
        rows_only.sort_unstable();
        println!(
            "scrollback {depth:>6}: grow {:>10.1} us   shrink {:>10.1} us   \
             rows-only round trip {:>8.3} us   (live rows {})",
            grow[RUNS / 2].as_secs_f64() * 1e6,
            shrink[RUNS / 2].as_secs_f64() * 1e6,
            rows_only[RUNS / 2].as_secs_f64() * 1e6,
            f.screen().newest().distance(f.screen().oldest()) + 1,
        );
    }
}
