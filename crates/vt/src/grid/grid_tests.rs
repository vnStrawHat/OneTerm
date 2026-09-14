//! The `grid::tests::` suite the design's Verification section names, plus the
//! four trap-map rows re-homed here from `cell::tests` because they need a grid
//! (traps 5, 7, 8 and the cross-row half of trap 6).

use super::*;
use crate::cell::{Attrs, Cell, CellContent, CellWidth, Color, Style};
use crate::intern::{Extras, GraphicId, Interner};
use crate::reflow::ResizePolicy;

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

    fn set_template(&mut self, template: Cell) {
        self.grid
            .screen_mut()
            .set_template(template, &mut self.interner);
    }

    fn set_style(&mut self, style: Style) {
        let id = self.interner.style(&style);
        self.set_template(Cell::EMPTY.with_style(id));
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
    f.grid
        .resize(Size { rows: 8, cols: 20 }, ResizePolicy::BottomAnchor);
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

    f.grid.resize(
        Size {
            rows: 60,
            cols: 200,
        },
        ResizePolicy::BottomAnchor,
    );
    f.grid
        .resize(Size { rows: 10, cols: 40 }, ResizePolicy::BottomAnchor);
    f.grid.resize(
        Size {
            rows: 4_000,
            cols: 9_000,
        },
        ResizePolicy::BottomAnchor,
    );

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

    f.grid.screen_mut().backspace(false);

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

    f.grid.screen_mut().backspace(false);

    assert_eq!(f.screen().cursor().pos.col, 0);
    assert!(
        f.screen().cursor().pending_wrap,
        "trap 1: BS at column 0 does not even clear the flag"
    );
    f.integrity();
}

#[test]
fn reverse_wrap_crosses_a_wrapped_row() {
    // Deviation D12 / R-08. `abcd` on a 4-column screen wraps, so row 0 is
    // WRAPPED and the cursor sits at row 1 column 0.
    let mut f = fixture(3, 4);
    f.print("abcde");
    f.goto(1, 0);
    assert!(f.screen().row(f.screen().row_of_index(0)).wrapped());

    // Reset (the default) is trap 1: nothing moves.
    f.grid.screen_mut().backspace(false);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (1, 0)
    );

    // Set: the cursor crosses into the wrapped row's last column.
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (0, 3)
    );
    f.integrity();

    // A row that is NOT wrapped is not crossed into, even with the mode set:
    // the glyph before the cursor is not there, so the previous line is
    // unrelated content.
    let mut f = fixture(3, 4);
    f.print("ab");
    f.newline();
    assert!(!f.screen().row(f.screen().row_of_index(0)).wrapped());
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (1, 0)
    );

    // The top of the screen is never crossed: history is not addressable.
    let mut f = fixture(3, 4);
    f.goto(0, 0);
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (0, 0)
    );
    f.integrity();
}

#[test]
fn reverse_wrap_stays_inside_the_scroll_region() {
    // `US-0087`, the cleanup row `migration.md` queued: the reference confines
    // reverse wrap to the scroll region — and so to the origin-mode region when
    // `DECOM` is set, which is the same region. Without the guard the cursor
    // escapes upwards into a row `DECSTBM` just told the program is not part of
    // the scrolling area.
    let mut f = fixture(4, 4);
    // Row 0 wraps into row 1, so the WRAPPED precondition (R-08) is met and the
    // only thing that can stop the crossing is the region.
    f.print("abcde");
    assert!(f.screen().row(f.screen().row_of_index(0)).wrapped());

    // Region rows 1..3: row 1 is its top, so `BS` there must not reach row 0.
    f.grid.screen_mut().set_region(1, 3, CursorOrigin::Screen);
    f.goto(1, 0);
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (1, 0),
        "BS at the region's top row must not cross above it"
    );
    f.integrity();

    // Inside the region it still crosses: the guard is the region's top, not a
    // blanket refusal. Row 1 wraps into row 2 here.
    f.goto(1, 0);
    f.print("wxyz1");
    assert!(f.screen().row(f.screen().row_of_index(1)).wrapped());
    f.goto(2, 0);
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (1, 3)
    );

    // Dropping the region back to the whole screen restores the screen-top
    // guard and nothing else, so row 1 reaches row 0 again.
    f.grid.screen_mut().set_region(0, 4, CursorOrigin::Screen);
    f.goto(1, 0);
    f.grid.screen_mut().backspace(true);
    assert_eq!(
        (f.screen().cursor_row_index(), f.screen().cursor().pos.col),
        (0, 3)
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
fn decawm_off_then_el0_erases_nothing() {
    // Deviation G3 is **withdrawn** (`US-0076` verification, M1). The flag is
    // armed at the last column whether or not `DECAWM` is set, exactly as the
    // reference arms `input_needs_wrap`; `DECAWM` is read at the wrap itself.
    // So `EL 0` behaves the same with the mode on or off — trap 2, unqualified.
    let mut f = fixture(2, 4);
    let no_wrap = PrintMode {
        insert: false,
        autowrap: false,
    };
    f.print_with(no_wrap, "abcd");
    assert!(
        f.screen().cursor().pending_wrap,
        "the flag is armed at the last column regardless of DECAWM"
    );
    assert_eq!(f.screen().cursor().pos.col, 3);
    assert_eq!(f.index_text(0), "abcd");

    f.grid.screen_mut().erase_line(LineClear::Right);

    assert_eq!(f.index_text(0), "abcd", "trap 2: EL 0 erases nothing");
    f.integrity();
}

#[test]
fn decawm_re_enabled_over_a_pending_wrap_still_breaks_the_line() {
    // The case deviation G3 claimed was unobservable: fill the row with the
    // mode off, turn it back on, print one glyph. The reference wraps, because
    // the flag it armed while the mode was off is still there. Gating the flag
    // on `DECAWM` lost the line break outright.
    let mut f = fixture(2, 4);
    let no_wrap = PrintMode {
        insert: false,
        autowrap: false,
    };
    f.print_with(no_wrap, "abcde");
    // With the mode off the fifth glyph overwrote the last column.
    assert_eq!(f.index_text(0), "abce");

    f.print("X");

    assert_eq!(f.index_text(0), "abce");
    assert_eq!(f.index_text(1), "X   ");
    assert_eq!(f.screen().cursor_row_index(), 1);
    assert_eq!(f.screen().cursor().pos.col, 1);
    f.integrity();
}

#[test]
fn pending_wrap_then_tab_wraps_and_returns() {
    let mut f = fixture(3, 12);
    f.print("abcdefghijkl");
    assert!(f.screen().cursor().pending_wrap);

    f.grid.put_tab(1, true);

    assert_eq!(f.screen().cursor_row_index(), 1, "trap 3: the line wrapped");
    assert_eq!(f.screen().cursor().pos.col, 0, "and the tab was consumed");
    f.integrity();
}

#[test]
fn decawm_off_then_tab_is_consumed_without_moving() {
    // Deviation G3 withdrawn (M1): the flag is armed either way, so `HT` takes
    // the reference's pending-wrap branch — which returns whether or not the
    // wrap happened. With `DECAWM` off the tab is simply eaten.
    let mut f = fixture(3, 12);
    let no_wrap = PrintMode {
        insert: false,
        autowrap: false,
    };
    f.print_with(no_wrap, "abcdefghijkl");
    assert!(f.screen().cursor().pending_wrap);

    f.grid.put_tab(1, false);

    assert_eq!(f.screen().cursor_row_index(), 0);
    assert_eq!(f.screen().cursor().pos.col, 11);
    assert!(
        f.screen().cursor().pending_wrap,
        "a consumed tab leaves the flag armed"
    );
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
    f.set_style(blue_underlined());
    let erase = f.screen().cursor().erase();
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
        f.screen().row(id).cells().iter().all(|cell| *cell == erase),
        "the background-erase cell fills the row"
    );

    // A reset with a different template cannot take the occ fast path, so the
    // whole row is rewritten.
    f.set_template(Cell::EMPTY);
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

/// A template that is wrong to erase with in every way: a foreground, an
/// attribute and a background that must survive.
fn blue_underlined() -> Style {
    Style {
        fg: Color::Palette(1),
        bg: Color::Palette(4),
        attrs: Attrs::UNDERLINE,
        ..Style::DEFAULT
    }
}

#[test]
fn erased_cells_keep_only_the_background() {
    let mut f = fixture(3, 8);
    f.set_style(blue_underlined());
    let background = f.interner.style(&Style {
        bg: Color::Palette(4),
        ..Style::DEFAULT
    });
    let expected = Cell::EMPTY.with_style(background);

    // Every in-row erase and every row reset fills with the background alone —
    // the reference's `bg.into()`. Each op is checked on a freshly printed row,
    // over the whole width, so nothing survives by accident.
    for op in ["ECH", "DCH", "ICH", "EL 2"] {
        f.goto(0, 0);
        f.print("abcdefgh");
        f.goto(0, 0);
        match op {
            "ECH" => f.grid.screen_mut().erase_chars(8),
            "DCH" => f.grid.screen_mut().delete_chars(8),
            "ICH" => f.grid.screen_mut().insert_blanks(8),
            _ => f.grid.screen_mut().erase_line(LineClear::All),
        }
        let row = f.screen().row_of_index(0);
        for (col, cell) in f.screen().row(row).cells().iter().enumerate() {
            assert_eq!(
                *cell, expected,
                "{op} left column {col} carrying more than the background"
            );
        }
    }

    // `ED 0` erases the cursor row's tail and resets every row below it.
    f.goto(2, 0);
    f.print("below");
    f.goto(1, 0);
    f.grid.erase_display(DisplayClear::Below, &f.interner);
    for index in 1..3 {
        let row = f.screen().row_of_index(index);
        assert!(
            f.screen()
                .row(row)
                .cells()
                .iter()
                .all(|cell| *cell == expected),
            "row {index} is filled the same way"
        );
    }

    assert_ne!(
        f.screen().cursor().template(),
        f.screen().cursor().erase(),
        "the printed template still carries fg, attrs and extras"
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

#[test]
fn entering_alt_screen_keeps_the_region_and_the_tab_stops() {
    let mut f = fixture(6, 24);
    f.grid.screen_mut().set_region(1, 4, CursorOrigin::Screen);
    f.grid.screen_mut().tabs_mut().clear(8);

    f.grid.swap_alt();

    assert_eq!(
        f.screen().region(),
        ScrollRegion { top: 1, bottom: 4 },
        "the reference keeps one scroll region for both screens, so entering is not a RIS"
    );
    assert!(
        !f.screen().tabs().is_stop(8),
        "nor does it restore tab stops"
    );

    // And the pair is shared in the other direction too.
    f.grid.screen_mut().set_region(2, 5, CursorOrigin::Screen);
    f.grid.swap_alt();
    assert_eq!(f.screen().region(), ScrollRegion { top: 2, bottom: 5 });
    f.integrity();
}

#[test]
fn entering_alt_screen_clears_with_the_background_template() {
    let mut f = fixture(4, 10);
    f.print("primary");
    f.set_style(blue_underlined());
    let erase = f.screen().cursor().erase();

    f.grid.swap_alt();

    let top = f.screen().row_of_index(0);
    assert!(
        f.screen()
            .row(top)
            .cells()
            .iter()
            .all(|cell| *cell == erase),
        "the entry wipe is a background erase with the entering template"
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

    f.grid
        .resize(Size { rows: 4, cols: 12 }, ResizePolicy::BottomAnchor);
    assert!(f.screen().tabs().is_stop(0));
    assert!(
        !f.screen().tabs().is_stop(8),
        "a cleared stop stays cleared"
    );

    f.grid
        .resize(Size { rows: 4, cols: 32 }, ResizePolicy::BottomAnchor);
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

/// The `US-0075` rework: a blanked row is a **changed** row, over both the
/// `place_row` path (an in-region scroll pulling an unwritten row over a written
/// one) and the `reset_row` empty-template path.
///
/// This is the grid half of `DEC-0015`'s "a second consumer becomes possible
/// without an engine change": everything here is read through the public
/// `row.seq()` / `row.flags()`, the way a consumer that is not `RenderState`
/// would have to.
#[test]
fn blanking_a_row_stamps_it_dirty_with_the_batch_seq() {
    for blank_with_a_scroll in [true, false] {
        let mut f = fixture(6, 10);
        // Row 3 is written; row 4 is left unwritten, so the scroll below pulls
        // an unallocated slot over an allocated one.
        let written = f.grid.begin_batch();
        f.goto(3, 0);
        f.print("row3");
        let id = f.screen().row_of_index(3);
        assert_eq!(f.screen().row(id).seq(), written);

        // A consumer renders here and takes the watermark with it.
        let watermark = written;

        let blanked = f.grid.begin_batch();
        if blank_with_a_scroll {
            // `place_row(.., None)`: row 4 is unwritten and moves onto row 3.
            f.grid.scroll_up(ScrollRegion { top: 1, bottom: 5 }, 1);
        } else {
            // `reset_row` with the default (empty) erase template.
            f.grid.screen_mut().reset_rows(3..4);
        }

        let row = f.screen().row(id);
        assert_eq!(trimmed(&f.row_text(id)), "", "the row was not blanked");
        assert!(
            row.seq() > watermark,
            "blanking left a stale stamp: {:?} is not above the watermark {watermark:?}",
            row.seq()
        );
        assert_eq!(row.seq(), blanked);
        assert!(
            row.flags().contains(RowFlags::DIRTY),
            "blanking left DIRTY clear, which is the false negative the \
             grapheme sweep and the graphics release scan must never see"
        );
        f.integrity();
    }
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
fn wide_char_wrapping_over_an_existing_pair_keeps_the_grid_intact() {
    // The last column already holds the `WideSpacer` of a pair, and the leading
    // spacer is about to be written over it. Writing that spacer without the
    // repair leaves the `Wide` at `cols - 2` orphaned — reachable in three
    // operations and caught by `assert_integrity`.
    let mut f = fixture(3, 8);
    f.goto(0, 6);
    f.print("\u{ff21}");
    let top = f.screen().row_of_index(0);
    assert_eq!(f.screen().row(top).cell(6).width(), CellWidth::Wide);
    assert_eq!(f.screen().row(top).cell(7).width(), CellWidth::WideSpacer);

    f.goto(0, 7);
    f.print("\u{ff22}");

    assert_eq!(
        f.screen().row(top).cell(6).width(),
        CellWidth::Narrow,
        "the orphaned half is released, not left behind"
    );
    assert_eq!(
        f.screen().row(top).cell(7).width(),
        CellWidth::LeadingWideSpacer
    );
    let next = f.screen().row_of_index(1);
    assert_eq!(f.screen().row(next).cell(0).width(), CellWidth::Wide);
    f.integrity();
}

#[test]
fn tab_does_not_orphan_a_wide_pair() {
    // Both spacer halves of a wide pair read as a space, so a tab that replaced
    // one with a narrow cell would leave its glyph orphaned. Found by the
    // property test once it could choose the column it printed at.
    let mut f = fixture(2, 8);
    f.goto(0, 6);
    f.print("\u{ff21}");
    f.goto(0, 7);

    f.grid.put_tab(1, true);

    let id = f.screen().row_of_index(0);
    assert_eq!(f.screen().row(id).cell(6).width(), CellWidth::Wide);
    assert_eq!(
        f.screen().row(id).cell(7).width(),
        CellWidth::WideSpacer,
        "the tab changed the content, not the width"
    );
    f.integrity();
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

    // For the ratio the packet records: the same scrollback once every row
    // carries text, so the two figures share a ring and are comparable.
    let mut written = fixture_with(45, 160, SCROLLBACK);
    for _ in 0..SCROLLBACK + 45 {
        written.goto(44, 0);
        written.print("x");
        written.newline();
    }
    let written_bytes = written.screen().heap_bytes();
    let written_per_row = written_bytes as f64 / f64::from(SCROLLBACK);
    let ring_share = ring as f64 * slot as f64 / f64::from(SCROLLBACK);
    let cells = written.screen().cols() as usize * size_of::<Cell>();
    println!(
        "written rows: {} allocated, {written_bytes} B total, {written_per_row:.1} B/row \
         ({ring_share:.1} B of ring share + {cells} B of cells)",
        written.screen().allocated_rows()
    );
    // Every row but the fresh blanks below the cursor carries cells now.
    assert!(written.screen().allocated_rows() >= SCROLLBACK as usize);
    f.integrity();
    written.integrity();
}
