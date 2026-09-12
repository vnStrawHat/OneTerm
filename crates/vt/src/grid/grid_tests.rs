//! The `grid::tests::` suite the design's Verification section names, plus the
//! four trap-map rows re-homed here from `cell::tests` because they need a grid
//! (traps 5, 7, 8 and the cross-row half of trap 6).

use super::*;
use crate::cell::{Cell, CellContent, CellWidth};
use crate::intern::{Extras, GraphicId, Interner};

struct Fixture {
    grid: TerminalGrid,
    interner: Interner,
}

fn fixture(rows: u16, cols: u16) -> Fixture {
    fixture_with(rows, cols, 100)
}

fn fixture_with(rows: u16, cols: u16, scrollback: u32) -> Fixture {
    Fixture {
        grid: TerminalGrid::new(Size { rows, cols }, scrollback),
        interner: Interner::default(),
    }
}

impl Fixture {
    fn screen(&self) -> &Screen {
        self.grid.screen()
    }

    fn print(&mut self, text: &str) {
        self.print_with(PrintMode::default(), text);
    }

    fn print_with(&mut self, mode: PrintMode, text: &str) {
        for c in text.chars() {
            self.grid.print(c, mode, &mut self.interner);
        }
    }

    /// `CR` + `LF`, the way a shell ends a line.
    fn newline(&mut self) {
        self.grid.screen_mut().carriage_return();
        self.grid.linefeed();
    }

    fn goto(&mut self, index: u16, col: u16) {
        self.grid.screen_mut().goto(index, col);
    }

    fn row_text(&self, id: RowId) -> String {
        let mut out = String::new();
        self.grid
            .screen_of(id)
            .row_text(id, &self.interner.graphemes, &mut out);
        out
    }

    fn index_text(&self, index: u16) -> String {
        let id = self.screen().row_of_index(index);
        self.row_text(id)
    }

    /// Write one labelled line per screen row, without scrolling.
    fn label_rows(&mut self) {
        for index in 0..self.screen().rows() {
            self.goto(index, 0);
            let label = format!("r{index}");
            self.print(&label);
        }
    }

    fn integrity(&self) {
        self.grid.assert_integrity(Some(&self.interner));
    }
}

fn trimmed(text: &str) -> String {
    text.trim_end().to_string()
}

// ── Viewport and identity ───────────────────────────────────────────────────

#[test]
fn offset_zero_follows_output() {
    let mut f = fixture(4, 10);
    for _ in 0..10 {
        f.newline();
    }
    assert_eq!(
        f.screen().scroll_offset(),
        0,
        "sticky bottom must stay sticky"
    );
    assert_eq!(f.screen().history_len(), 7);
    assert_eq!(f.screen().viewport().top, f.screen().screen_top());
    f.integrity();
}

#[test]
fn scrolled_back_view_holds_still_while_rows_are_pushed() {
    let mut f = fixture(4, 10);
    for _ in 0..10 {
        f.newline();
    }
    f.grid.screen_mut().scroll_viewport(-3);
    assert_eq!(f.screen().scroll_offset(), 3);
    let held = f.screen().viewport().top;

    for _ in 0..2 {
        f.newline();
    }

    assert_eq!(
        f.screen().scroll_offset(),
        5,
        "the offset grows with the output"
    );
    assert_eq!(
        f.screen().viewport().top,
        held,
        "the user keeps looking at the same content"
    );
    f.integrity();
}

#[test]
fn offset_is_capped_by_history() {
    let mut f = fixture(4, 10);
    for _ in 0..10 {
        f.newline();
    }
    f.grid.screen_mut().scroll_viewport(-10_000);
    assert_eq!(f.screen().scroll_offset(), f.screen().history_len());
    assert_eq!(f.screen().viewport().top, f.screen().oldest());

    f.grid.screen_mut().scroll_to_bottom();
    assert_eq!(f.screen().scroll_offset(), 0);
    f.integrity();
}

#[test]
fn row_ids_are_monotonic_across_scroll_clear_and_ris() {
    let mut f = fixture(4, 10);
    f.print("hello");
    let after_print = f.screen().newest();

    for _ in 0..6 {
        f.newline();
    }
    let after_scroll = f.screen().newest();
    assert!(after_scroll > after_print, "a scroll allocates fresh ids");

    f.grid
        .erase_display(DisplayClear::All, &Interner::default());
    assert!(
        f.screen().newest() >= after_scroll,
        "ED 2 never rewinds ids"
    );
    let after_ed2 = f.screen().newest();

    f.grid
        .erase_display(DisplayClear::Saved, &Interner::default());
    assert_eq!(f.screen().newest(), after_ed2, "ED 3 does not touch ids");

    f.grid.reset();
    assert_eq!(f.screen().newest(), after_ed2, "RIS does not reset ids");
    f.integrity();
}

#[test]
fn the_two_screens_never_share_a_row_id() {
    let mut f = fixture(4, 10);
    for _ in 0..20 {
        f.newline();
    }
    f.grid.swap_alt();
    for _ in 0..20 {
        f.newline();
    }

    let primary = f.grid.primary().row_range();
    let alt = f.grid.alt().row_range();
    assert!(
        primary.end <= alt.start || alt.end <= primary.start,
        "the two runs overlap: {primary:?} and {alt:?}"
    );
    f.integrity();
}

#[test]
fn lines_produced_counts_output_lines_not_wraps() {
    let mut f = fixture(4, 5);
    f.print("abcdefghij");
    assert_eq!(
        f.grid.lines_produced(),
        0,
        "an implicit wrap is not an output line"
    );

    for _ in 0..3 {
        f.newline();
    }
    assert_eq!(f.grid.lines_produced(), 3);

    f.grid.scroll_up(ScrollRegion::full(4), 2);
    assert_eq!(
        f.grid.lines_produced(),
        5,
        "a region scroll that fills history counts its rows"
    );

    f.grid.scroll_down(ScrollRegion::full(4), 2);
    assert_eq!(f.grid.lines_produced(), 5, "SD never touches history");
    f.integrity();
}

#[test]
fn lines_produced_is_unchanged_by_reflow_and_by_clear() {
    let mut f = fixture(4, 10);
    for _ in 0..5 {
        f.newline();
    }
    let produced = f.grid.lines_produced();

    f.grid
        .erase_display(DisplayClear::All, &Interner::default());
    assert_eq!(f.grid.lines_produced(), produced, "ED 2");
    f.grid
        .erase_display(DisplayClear::Saved, &Interner::default());
    assert_eq!(f.grid.lines_produced(), produced, "ED 3");
    f.grid.swap_alt();
    f.grid.swap_alt();
    assert_eq!(f.grid.lines_produced(), produced, "alternate-screen swap");
    f.grid.resize(Size { rows: 8, cols: 20 });
    assert_eq!(f.grid.lines_produced(), produced, "resize");
    f.grid.reset();
    assert_eq!(f.grid.lines_produced(), produced, "RIS");
    f.integrity();
}

#[test]
fn unwritten_slots_read_as_blanks() {
    let f = fixture(4, 10);
    assert_eq!(
        f.screen().allocated_rows(),
        0,
        "a fresh screen holds no cells"
    );
    let mut id = f.screen().oldest();
    while id <= f.screen().newest() {
        let row = f.screen().row(id);
        assert!(!row.is_allocated());
        assert!(row.cells().iter().all(|cell| *cell == Cell::EMPTY));
        assert_eq!(f.row_text(id), "          ");
        id = id + 1;
    }
    f.integrity();
}

#[test]
fn ring_mask_is_constant_across_resizes() {
    let mut f = fixture_with(24, 80, 10_000);
    let mask = f.screen().ring_mask();
    let ring_len = f.screen().ring_len();
    assert_eq!(ring_len, 16_384, "next_power_of_two(10_000 + MAX_ROWS)");

    f.grid.resize(Size {
        rows: 60,
        cols: 200,
    });
    f.grid.resize(Size { rows: 10, cols: 40 });
    f.grid.resize(Size {
        rows: 4_000,
        cols: 9_000,
    });

    assert_eq!(f.screen().ring_mask(), mask);
    assert_eq!(f.screen().ring_len(), ring_len);
    assert_eq!(f.screen().rows(), MAX_ROWS, "an oversized resize clamps");
    assert_eq!(f.screen().cols(), MAX_COLS);
    f.integrity();
}

#[test]
fn changing_the_scrollback_limit_rehomes_live_rows() {
    let mut f = fixture_with(4, 10, 10_000);
    let before = f.screen().ring_len();
    for step in 0..20 {
        f.goto(3, 0);
        let label = format!("line{step}");
        f.print(&label);
        f.newline();
    }
    let oldest_text = f.row_text(f.screen().newest() - 5);

    f.grid.set_scrollback_limit(50);

    assert_ne!(f.screen().ring_len(), before, "the ring is rehomed");
    assert_eq!(
        f.screen().ring_len(),
        (50 + MAX_ROWS as usize).next_power_of_two()
    );
    assert_eq!(
        f.row_text(f.screen().newest() - 5),
        oldest_text,
        "live rows survive the rehome"
    );
    f.integrity();
}

// ── Pending wrap ────────────────────────────────────────────────────────────

#[test]
fn pending_wrap_then_backspace() {
    let mut f = fixture(2, 4);
    f.print("abcd");
    assert!(f.screen().cursor().pending_wrap);
    assert_eq!(f.screen().cursor().pos.col, 3);

    f.grid.screen_mut().backspace();

    assert!(!f.screen().cursor().pending_wrap);
    assert_eq!(f.screen().cursor().pos.col, 2);
    f.integrity();
}

#[test]
fn backspace_at_column_zero_is_a_noop() {
    let mut f = fixture(2, 4);
    f.grid.screen_mut().cursor_mut().pending_wrap = true;
    f.goto(0, 0);
    f.grid.screen_mut().cursor_mut().pending_wrap = true;

    f.grid.screen_mut().backspace();

    assert_eq!(f.screen().cursor().pos.col, 0);
    assert!(
        f.screen().cursor().pending_wrap,
        "trap 1: BS at column 0 does not even clear the flag"
    );
    f.integrity();
}

#[test]
fn pending_wrap_then_el0_erases_nothing() {
    let mut f = fixture(2, 4);
    f.print("abcd");
    assert!(f.screen().cursor().pending_wrap);

    f.grid.screen_mut().erase_line(LineClear::Right);

    assert_eq!(f.index_text(0), "abcd", "trap 2");
    f.integrity();
}

#[test]
fn decawm_off_then_el0_erases() {
    let mut f = fixture(2, 4);
    let no_wrap = PrintMode {
        insert: false,
        autowrap: false,
    };
    f.print_with(no_wrap, "abcd");
    assert!(
        !f.screen().cursor().pending_wrap,
        "deviation G3: the flag is not armed with DECAWM reset"
    );
    assert_eq!(f.screen().cursor().pos.col, 3, "the column clamps instead");

    f.grid.screen_mut().erase_line(LineClear::Right);

    assert_eq!(f.index_text(0), "abc ", "G3's observable half: EL 0 erases");
    f.integrity();
}

#[test]
fn pending_wrap_then_tab_wraps_and_returns() {
    let mut f = fixture(3, 12);
    f.print("abcdefghijkl");
    assert!(f.screen().cursor().pending_wrap);

    f.grid.put_tab(1);

    assert_eq!(f.screen().cursor_row_index(), 1, "trap 3: the line wrapped");
    assert_eq!(f.screen().cursor().pos.col, 0, "and the tab was consumed");
    f.integrity();
}

#[test]
fn decawm_off_then_tab_moves_to_the_next_stop() {
    let mut f = fixture(3, 12);
    let no_wrap = PrintMode {
        insert: false,
        autowrap: false,
    };
    f.print_with(no_wrap, "abcdefghijkl");
    assert!(!f.screen().cursor().pending_wrap);

    f.grid.put_tab(1);

    // G3's second half: the reference would consume a stuck wrap and move the
    // cursor to the next row. Here HT runs its own rule, which stops at the last
    // column, so the cursor never leaves its row.
    assert_eq!(f.screen().cursor_row_index(), 0);
    assert_eq!(f.screen().cursor().pos.col, 11);
    f.integrity();
}

// ── Erase ───────────────────────────────────────────────────────────────────

#[test]
fn ed2_scrolls_the_viewport_into_history() {
    let mut f = fixture(4, 10);
    f.print("hello");
    let text_row = f.screen().cursor().pos.row;

    f.grid
        .erase_display(DisplayClear::All, &Interner::default());

    assert_eq!(f.screen().history_len(), 1, "trap 9: the content is kept");
    assert_eq!(trimmed(&f.row_text(text_row)), "hello");
    for index in 0..4 {
        assert_eq!(trimmed(&f.index_text(index)), "", "the screen is blank");
    }
    f.integrity();
}

#[test]
fn ed2_keeps_the_scrolled_back_viewport_position() {
    let mut f = fixture(4, 10);
    for step in 0..10 {
        f.goto(3, 0);
        let label = format!("l{step}");
        f.print(&label);
        f.newline();
    }
    f.grid.screen_mut().scroll_viewport(-4);
    let held = f.screen().viewport().top;
    let held_text = f.row_text(held);

    f.grid
        .erase_display(DisplayClear::All, &Interner::default());

    assert_eq!(
        f.screen().viewport().top,
        held,
        "trap 9: the scrolled-back user keeps seeing the same content"
    );
    assert_eq!(f.row_text(held), held_text);
    f.integrity();
}

#[test]
fn ed2_over_an_image_keeps_the_image_rows() {
    let mut f = fixture(4, 10);
    let extras = f.interner.extras(&Extras {
        hyperlink: None,
        graphic: Some(GraphicId(1)),
    });
    // A cell covered by an image is a space, but it is not erasable (R-13).
    let covered = Cell::EMPTY.with_extras(extras);
    let id = f.screen().row_of_index(2);
    f.grid.screen_mut().row_mut(id).set(0, covered);
    f.grid.screen_mut().row_mut(id).mark_graphic();

    f.grid.erase_display(DisplayClear::All, &f.interner);

    assert_eq!(
        f.screen().history_len(),
        3,
        "the image rows were scrolled into history, not discarded"
    );
    assert_eq!(f.grid.screen_of(id).row(id).cell(0), covered);
    f.integrity();
}

#[test]
fn ed3_resets_the_viewport_to_the_bottom() {
    let mut f = fixture(4, 10);
    for _ in 0..10 {
        f.newline();
    }
    f.grid.screen_mut().scroll_viewport(-5);
    assert!(f.screen().scroll_offset() > 0);

    f.grid
        .erase_display(DisplayClear::Saved, &Interner::default());

    assert_eq!(f.screen().scroll_offset(), 0, "trap 10");
    assert_eq!(f.screen().history_len(), 0);
    f.integrity();
}

#[test]
fn ed1_clears_row_zero() {
    let mut f = fixture(4, 10);
    f.goto(0, 0);
    f.print("aaaaa");
    f.goto(1, 0);
    f.print("bbbbb");
    f.goto(1, 1);

    f.grid
        .erase_display(DisplayClear::Above, &Interner::default());

    assert_eq!(
        trimmed(&f.index_text(0)),
        "",
        "correction C2: row 0 is cleared, where the reference's guard skips it"
    );
    assert_eq!(f.index_text(1), "  bbb     ");
    f.integrity();
}

#[test]
fn row_reset_respects_occ_and_the_background_template() {
    let mut f = fixture(2, 8);
    let blue = f.interner.style(&crate::cell::Style {
        bg: crate::cell::Color::Palette(4),
        ..crate::cell::Style::DEFAULT
    });
    let template = Cell::EMPTY.with_style(blue);
    f.grid.screen_mut().cursor_mut().template = template;
    f.goto(0, 0);
    f.print("xyz");
    let id = f.screen().row_of_index(0);
    assert_eq!(f.screen().row(id).occ(), 3, "occ tracks what was written");

    f.grid.screen_mut().reset_rows(0..1);

    assert_eq!(
        f.screen().row(id).occ(),
        0,
        "trap 36: a reset leaves nothing written"
    );
    assert!(
        f.screen()
            .row(id)
            .cells()
            .iter()
            .all(|cell| *cell == template),
        "the background-erase template fills the row"
    );

    // A reset with a different template cannot take the occ fast path, so the
    // whole row is rewritten.
    f.grid.screen_mut().cursor_mut().template = Cell::EMPTY;
    f.grid.screen_mut().reset_rows(0..1);
    assert!(
        f.screen()
            .row(id)
            .cells()
            .iter()
            .all(|cell| *cell == Cell::EMPTY)
    );
    f.integrity();
}

// ── Alternate screen ────────────────────────────────────────────────────────

#[test]
fn entering_alt_screen_overwrites_the_saved_cursor() {
    let mut f = fixture(4, 10);
    f.goto(2, 3);
    f.grid.screen_mut().save_cursor();
    f.goto(0, 1);

    f.grid.swap_alt();
    f.grid.swap_alt();
    f.grid.screen_mut().restore_cursor();

    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (0, 1),
        "trap 14: ? 1049 h takes the primary DECSC slot"
    );
    f.integrity();
}

#[test]
fn leaving_alt_screen_restores_the_entry_cursor() {
    let mut f = fixture(4, 10);
    f.goto(2, 3);

    f.grid.swap_alt();
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (2, 3),
        "the alternate screen starts at the primary cursor's INDEX"
    );
    f.goto(0, 0);
    f.print("alt");
    f.grid.swap_alt();

    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (2, 3)
    );
    f.integrity();
}

// ── Scroll regions ──────────────────────────────────────────────────────────

#[test]
fn linefeed_below_the_region_does_not_scroll() {
    let mut f = fixture(6, 10);
    f.grid.screen_mut().set_region(1, 3, CursorOrigin::Screen);
    f.goto(4, 0);
    let newest = f.screen().newest();

    f.grid.linefeed();
    assert_eq!(f.screen().cursor_row_index(), 5);
    f.grid.linefeed();

    assert_eq!(
        f.screen().cursor_row_index(),
        5,
        "trap 16: it stops at the bottom"
    );
    assert_eq!(f.screen().newest(), newest, "and never scrolls");
    f.integrity();
}

#[test]
fn insert_and_delete_lines_outside_the_region_are_noops() {
    let mut f = fixture(6, 10);
    f.label_rows();
    f.grid.screen_mut().set_region(1, 3, CursorOrigin::Screen);
    f.goto(4, 0);
    let before: Vec<String> = (0..6).map(|index| f.index_text(index)).collect();

    assert!(f.grid.insert_lines(2).is_none());
    assert!(f.grid.delete_lines(2).is_none());

    let after: Vec<String> = (0..6).map(|index| f.index_text(index)).collect();
    assert_eq!(before, after, "trap 16");
    f.integrity();
}

#[test]
fn scroll_up_fills_history_only_from_row_zero() {
    let mut f = fixture(6, 10);
    let history = f.screen().history_len();

    f.grid.scroll_up(ScrollRegion { top: 2, bottom: 5 }, 1);
    assert_eq!(
        f.screen().history_len(),
        history,
        "trap 17: a region above row 0 never fills scrollback"
    );

    f.grid.scroll_up(ScrollRegion { top: 0, bottom: 5 }, 1);
    assert_eq!(f.screen().history_len(), history + 1);
    f.integrity();
}

#[test]
fn scroll_up_with_a_bottom_bounded_region_keeps_the_rows_below() {
    let mut f = fixture(5, 10);
    f.label_rows();

    f.grid.scroll_up(ScrollRegion { top: 0, bottom: 3 }, 1);

    assert_eq!(trimmed(&f.index_text(0)), "r1");
    assert_eq!(trimmed(&f.index_text(1)), "r2");
    assert_eq!(
        trimmed(&f.index_text(2)),
        "",
        "the region's bottom row is blank"
    );
    assert_eq!(
        trimmed(&f.index_text(3)),
        "r3",
        "R-03: the rows below stay put"
    );
    assert_eq!(trimmed(&f.index_text(4)), "r4");
    assert_eq!(f.screen().history_len(), 1);
    assert_eq!(trimmed(&f.row_text(f.screen().screen_top() - 1)), "r0");
    f.integrity();
}

#[test]
fn small_region_scroll_rotates_then_blanks() {
    let mut f = fixture(5, 10);
    f.label_rows();
    let history = f.screen().history_len();
    let newest = f.screen().newest();

    // The count is far larger than the region, so the region rotates by its own
    // height and is then blank — correction C3, reached by the same path as
    // every other count.
    f.grid.scroll_up(ScrollRegion { top: 1, bottom: 3 }, 9);

    assert_eq!(trimmed(&f.index_text(0)), "r0");
    assert_eq!(trimmed(&f.index_text(1)), "");
    assert_eq!(trimmed(&f.index_text(2)), "");
    assert_eq!(trimmed(&f.index_text(3)), "r3");
    assert_eq!(trimmed(&f.index_text(4)), "r4");
    assert_eq!(f.screen().history_len(), history, "nothing entered history");
    assert_eq!(f.screen().newest(), newest, "and no id was allocated");
    f.integrity();
}

#[test]
fn scroll_down_never_pulls_from_history() {
    let mut f = fixture(4, 10);
    for step in 0..8 {
        f.goto(3, 0);
        let label = format!("l{step}");
        f.print(&label);
        f.newline();
    }
    let history = f.screen().history_len();

    f.grid.scroll_down(ScrollRegion::full(4), 2);

    assert_eq!(f.screen().history_len(), history, "trap 18");
    assert_eq!(trimmed(&f.index_text(0)), "");
    assert_eq!(trimmed(&f.index_text(1)), "");
    f.integrity();
}

#[test]
fn decstbm_invalid_range_is_a_noop_and_valid_homes_the_cursor() {
    let mut f = fixture(6, 10);
    assert!(f.grid.screen_mut().set_region(1, 4, CursorOrigin::Screen));
    assert_eq!(f.screen().region(), ScrollRegion { top: 1, bottom: 4 });
    assert_eq!(f.screen().cursor_row_index(), 0, "a valid DECSTBM homes");

    f.goto(3, 3);
    assert!(
        !f.grid.screen_mut().set_region(4, 4, CursorOrigin::Screen),
        "trap 15: top >= bottom is refused"
    );
    assert_eq!(
        f.screen().region(),
        ScrollRegion { top: 1, bottom: 4 },
        "and the previous region survives"
    );
    assert_eq!(f.screen().cursor_row_index(), 3, "the cursor is not homed");

    assert!(f.grid.screen_mut().set_region(2, 5, CursorOrigin::Region));
    assert_eq!(
        f.screen().cursor_row_index(),
        2,
        "DECSTBM homes origin-relative"
    );
    f.integrity();
}

// ── Erase, insert, delete inside a row ──────────────────────────────────────

#[test]
fn delete_chars_shifts_left_by_n() {
    let mut f = fixture(1, 10);
    f.print("abcdefghij");
    f.goto(0, 2);
    f.grid.screen_mut().delete_chars(3);
    assert_eq!(f.index_text(0), "abfghij   ");

    f.goto(0, 0);
    f.print("abcdefghij");
    f.goto(0, 2);
    // Correction C1: a count at or beyond the row width is still a plain shift,
    // where the reference's `end` clamp turns it into something else.
    f.grid.screen_mut().delete_chars(40);
    assert_eq!(f.index_text(0), "ab        ");
    f.integrity();
}

#[test]
fn tab_stops_survive_and_regrow_across_a_resize() {
    let mut f = fixture(4, 24);
    f.grid.screen_mut().tabs_mut().clear(8);
    assert!(!f.screen().tabs().is_stop(8));

    f.grid.resize(Size { rows: 4, cols: 12 });
    assert!(f.screen().tabs().is_stop(0));
    assert!(
        !f.screen().tabs().is_stop(8),
        "a cleared stop stays cleared"
    );

    f.grid.resize(Size { rows: 4, cols: 32 });
    assert!(
        !f.screen().tabs().is_stop(8),
        "trap 27: the prefix is retained"
    );
    assert!(
        f.screen().tabs().is_stop(16),
        "the regrown region gets defaults"
    );
    assert!(f.screen().tabs().is_stop(24));
    f.integrity();
}

#[test]
fn first_visible_cell_is_viewport_top_column_zero() {
    let mut f = fixture(4, 10);
    for step in 0..10 {
        f.goto(3, 0);
        let label = format!("l{step}");
        f.print(&label);
        f.newline();
    }
    f.grid.screen_mut().scroll_viewport(-3);

    let viewport = f.screen().viewport();
    assert_eq!(viewport.top, f.screen().visible_top());
    assert_eq!(
        f.screen().row(viewport.top).cell(0).content(),
        CellContent::Scalar('l'),
        "trap 46: the first visible cell is (viewport top, column 0)"
    );
    f.integrity();
}

#[test]
fn trimmed_slot_is_cleared_before_reuse() {
    let mut f = fixture_with(2, 10, 2);
    f.goto(0, 0);
    f.print("keepme");
    let ring = f.screen().ring_len() as u64;

    // Walk the whole ring so every slot is reused at least once.
    for _ in 0..ring + 5 {
        f.newline();
    }

    let mut id = f.screen().oldest();
    while id <= f.screen().newest() {
        let row = f.screen().row(id);
        assert_eq!(row.id(), id, "a reused slot must carry its new id");
        assert_eq!(trimmed(&f.row_text(id)), "", "and none of the old content");
        id = id + 1;
    }
    f.integrity();
}

// ── Print path: the trap-map rows re-homed from cell::tests ─────────────────

#[test]
fn wide_char_at_last_column_wrap_on_and_off() {
    let mut f = fixture(3, 4);
    f.goto(0, 3);
    f.print("\u{ff21}"); // fullwidth A

    let top = f.screen().row_of_index(0);
    assert_eq!(
        f.screen().row(top).cell(3).width(),
        CellWidth::LeadingWideSpacer,
        "trap 5: the glyph does not fit, so a spacer holds its place"
    );
    assert!(f.screen().row(top).wrapped());
    let next = f.screen().row_of_index(1);
    assert_eq!(f.screen().row(next).cell(0).width(), CellWidth::Wide);
    assert_eq!(f.screen().row(next).cell(1).width(), CellWidth::WideSpacer);
    f.integrity();

    let mut g = fixture(3, 4);
    g.goto(0, 3);
    g.print_with(
        PrintMode {
            insert: false,
            autowrap: false,
        },
        "\u{ff21}",
    );
    let top = g.screen().row_of_index(0);
    assert_eq!(
        g.screen().row(top).cell(3),
        Cell::EMPTY,
        "the glyph is dropped"
    );
    assert_eq!(g.screen().dropped_wide(), 1);
    assert!(!g.screen().row(top).wrapped());
    g.integrity();
}

#[test]
fn wide_pair_repair_across_rows() {
    let mut f = fixture(3, 4);
    f.goto(0, 3);
    f.print("\u{ff21}");
    let top = f.screen().row_of_index(0);
    assert_eq!(
        f.screen().row(top).cell(3).width(),
        CellWidth::LeadingWideSpacer
    );

    // Writing over the glyph on the next row releases the previous row's
    // trailing spacer: trap 6's cross-row half.
    f.goto(1, 0);
    f.print("x");

    assert_eq!(f.screen().row(top).cell(3).width(), CellWidth::Narrow);
    assert_eq!(
        f.screen().row(top).cell(3).content(),
        CellContent::Scalar(' ')
    );
    let next = f.screen().row_of_index(1);
    assert_eq!(
        f.screen().row(next).cell(0).content(),
        CellContent::Scalar('x')
    );
    assert_eq!(
        f.screen().row(next).cell(1).width(),
        CellWidth::Narrow,
        "the orphaned spacer is repaired too"
    );
    f.integrity();
}

#[test]
fn insert_mode_over_wide_char_repairs_the_pair() {
    let mut f = fixture(1, 6);
    f.print("\u{ff21}b");
    let id = f.screen().row_of_index(0);
    assert_eq!(f.screen().row(id).cell(0).width(), CellWidth::Wide);
    assert_eq!(f.screen().row(id).cell(1).width(), CellWidth::WideSpacer);

    // Insert onto the spacer: the reference leaves the `Wide` half orphaned.
    f.goto(0, 1);
    f.print_with(
        PrintMode {
            insert: true,
            autowrap: true,
        },
        "x",
    );

    let cells = f.screen().row(id).cells().to_vec();
    assert!(
        cells.iter().all(
            |cell| cell.width() != CellWidth::WideSpacer || cells[0].width() == CellWidth::Wide
        ),
        "correction C4: no orphaned spacer survives"
    );
    assert_eq!(cells[1].content(), CellContent::Scalar('x'));
    f.integrity();
}

#[test]
fn zero_width_at_column_zero_attaches_to_column_zero() {
    let mut f = fixture(1, 5);
    f.goto(0, 0);
    f.print("\u{0301}"); // combining acute accent

    let id = f.screen().row_of_index(0);
    assert!(
        matches!(
            f.screen().row(id).cell(0).content(),
            CellContent::Grapheme(_)
        ),
        "trap 8: at column 0 with no pending wrap it attaches to column 0"
    );
    assert_eq!(f.row_text(id), " \u{0301}    ");
    assert_eq!(f.screen().cursor().pos.col, 0, "and takes no column");
    f.integrity();
}

#[test]
fn wide_char_dropped_when_cols_is_one() {
    let mut f = fixture(2, 1);
    f.print("\u{ff21}");

    assert_eq!(f.screen().dropped_wide(), 1);
    let id = f.screen().row_of_index(0);
    assert_eq!(f.screen().row(id).cell(0), Cell::EMPTY);
    f.integrity();
}

// ── Memory ──────────────────────────────────────────────────────────────────

/// The physical half of the design's memory claim: a 100 000-row scrollback of
/// rows that were never written costs its ring slots and nothing else.
///
/// The old engine measured 4138 bytes per 160-column row for every content kind,
/// because it materialised every row (`evidence/US-0072-bench-baseline.md`).
#[test]
fn empty_scrollback_costs_only_its_ring_slots() {
    const SCROLLBACK: u32 = 100_000;
    let mut f = fixture_with(45, 160, SCROLLBACK);

    for _ in 0..SCROLLBACK + 45 {
        f.grid.linefeed();
    }
    assert_eq!(f.screen().history_len(), SCROLLBACK);
    assert_eq!(
        f.screen().allocated_rows(),
        0,
        "not one row of cells was materialised"
    );

    let slot = size_of::<Option<Row>>();
    assert_eq!(
        slot,
        size_of::<Row>(),
        "the Option is niche-packed into the Vec"
    );
    let ring = f.screen().ring_len();
    let bytes = f.screen().heap_bytes();
    let per_row = bytes as f64 / f64::from(SCROLLBACK);
    println!(
        "empty scrollback: {SCROLLBACK} rows, ring {ring} slots x {slot} B = {bytes} B total, \
         {per_row:.1} B/row (slot itself {slot} B); old engine: 4138 B/row"
    );

    assert!(
        bytes >= ring * slot && bytes < ring * slot + 4_096,
        "the ring's slots are the whole cost: {bytes} B against {} B of slots",
        ring * slot
    );
    assert!(
        per_row < 100.0,
        "{per_row} B/row is not the lazily allocated figure"
    );

    // For the ratio the packet records: the same rows once they carry text.
    let mut written = fixture_with(45, 160, 1_000);
    for _ in 0..1_000 {
        written.goto(44, 0);
        written.print("x");
        written.newline();
    }
    let written_bytes = written.screen().heap_bytes();
    println!(
        "written rows: {} allocated, {written_bytes} B total, {:.1} B/row",
        written.screen().allocated_rows(),
        written_bytes as f64 / 1_000.0
    );
    f.integrity();
}
