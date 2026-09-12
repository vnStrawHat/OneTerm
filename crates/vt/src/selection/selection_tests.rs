//! `selection::tests::` — the verification list in
//! `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md`, plus the
//! outcome list in `US-0078-selection.md`.
//!
//! The reference's own cases are in
//! `vendor/alacritty_terminal/src/selection.rs:400-668`; they are reproduced
//! here against OneTerm's types rather than copied, because the coordinates
//! differ (absolute [`RowId`] rather than a viewport `Line`).

use super::*;
use crate::cell::CellContent;
use crate::grid::{PrintMode, RowId, ScrollRegion, Size, TerminalGrid};
use crate::intern::Interner;
use crate::reflow::ResizePolicy;

struct Fixture {
    grid: TerminalGrid,
    interner: Interner,
}

fn fixture(rows: u16, cols: u16, scrollback: u32) -> Fixture {
    Fixture {
        grid: TerminalGrid::new(Size { rows, cols }, scrollback),
        interner: Interner::default(),
    }
}

impl Fixture {
    fn feed(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\r' => self.grid.screen_mut().carriage_return(),
                '\n' => {
                    self.grid.linefeed();
                }
                '\t' => self.grid.put_tab(1, true),
                _ => self.grid.print(c, PrintMode::default(), &mut self.interner),
            }
        }
    }

    /// A screen row by index; a negative index reaches into history.
    fn row_id(&self, index: i32) -> RowId {
        let top = self.grid.screen().screen_top();
        if index < 0 {
            top - index.unsigned_abs() as u64
        } else {
            top + index as u64
        }
    }

    fn at(&self, index: i32, col: u16) -> Pos {
        Pos {
            row: self.row_id(index),
            col,
        }
    }

    fn select(&mut self, kind: SelectionKind, from: (Pos, Side), to: (Pos, Side)) -> Selection {
        let mut selection = Selection::new(&mut self.grid, kind, from.0, from.1);
        selection.update(&mut self.grid, to.0, to.1);
        selection
    }

    /// Exactly the cells `from..=to`: the left edge of the first and the right
    /// edge of the last.
    fn span(&mut self, kind: SelectionKind, from: Pos, to: Pos) -> Selection {
        self.select(kind, (from, Side::Left), (to, Side::Right))
    }

    fn range(&self, selection: &Selection) -> Option<SelectionRange> {
        selection.to_range(&self.grid, SEMANTIC_ESCAPE_CHARS)
    }

    fn text(&self, selection: &Selection) -> Option<String> {
        selection.text(&self.grid, &self.interner, SEMANTIC_ESCAPE_CHARS)
    }

    fn text_with(&self, selection: &Selection, escape_chars: &str) -> Option<String> {
        selection.text(&self.grid, &self.interner, escape_chars)
    }

    fn goto(&mut self, index: u16, col: u16) {
        self.grid.screen_mut().goto(index, col);
    }
}

// ── `Simple` ────────────────────────────────────────────────────────────────

#[test]
fn simple_side_rules_drop_the_right_cells() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij");

    let cases = [
        (Side::Left, Side::Right, "cdef"),
        (Side::Right, Side::Left, "de"),
        (Side::Left, Side::Left, "cde"),
        (Side::Right, Side::Right, "def"),
    ];
    for (start_side, end_side, expected) in cases {
        let (start, end) = (f.at(0, 2), f.at(0, 5));
        let selection = f.select(SelectionKind::Simple, (start, start_side), (end, end_side));
        assert_eq!(
            f.text(&selection).as_deref(),
            Some(expected),
            "{start_side:?} -> {end_side:?}"
        );
        selection.release(&mut f.grid);
    }
}

#[test]
fn simple_is_empty_when_anchors_coincide_or_are_adjacent() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij");

    // Identical anchors, identical sides.
    let pos = f.at(0, 3);
    let selection = f.select(SelectionKind::Simple, (pos, Side::Left), (pos, Side::Left));
    assert_eq!(f.range(&selection), None);
    selection.release(&mut f.grid);

    // Adjacent cells, right -> left.
    let selection = f.select(
        SelectionKind::Simple,
        (f.at(0, 3), Side::Right),
        (f.at(0, 4), Side::Left),
    );
    assert_eq!(f.range(&selection), None);
    selection.release(&mut f.grid);

    // The same drag backwards is still empty: the anchors are ordered first.
    let selection = f.select(
        SelectionKind::Simple,
        (f.at(0, 4), Side::Left),
        (f.at(0, 3), Side::Right),
    );
    assert_eq!(f.range(&selection), None);
    selection.release(&mut f.grid);

    // One whole cell is not empty.
    let selection = f.select(
        SelectionKind::Simple,
        (f.at(0, 3), Side::Left),
        (f.at(0, 3), Side::Right),
    );
    assert_eq!(f.text(&selection).as_deref(), Some("d"));
}

/// The reference's own asymmetry, reproduced deliberately: the end is trimmed
/// before the start is, so by the time the start would be dropped the two
/// coincide and it is kept. The same drag *inside* one row is empty.
#[test]
fn the_last_half_cell_of_a_row_through_the_first_of_the_next_keeps_that_cell() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst");

    let selection = f.select(
        SelectionKind::Simple,
        (f.at(0, 9), Side::Right),
        (f.at(1, 0), Side::Left),
    );
    let range = f
        .range(&selection)
        .expect("the reference keeps the last cell");
    assert_eq!((range.start, range.end), (f.at(0, 9), f.at(0, 9)));
    assert_eq!(f.text(&selection).as_deref(), Some("j"));
}

#[test]
fn reversed_anchors_produce_the_same_range_for_every_kind() {
    let mut f = fixture(5, 10, 100);
    f.feed("abc def\r\nghi jkl\r\nmno pqr");

    for kind in [
        SelectionKind::Simple,
        SelectionKind::Block,
        SelectionKind::Semantic,
        SelectionKind::Lines,
    ] {
        let (low, high) = (f.at(0, 2), f.at(2, 5));
        let forward = f.select(kind, (low, Side::Left), (high, Side::Right));
        let forward_range = f.range(&forward);
        forward.release(&mut f.grid);

        let backward = f.select(kind, (high, Side::Right), (low, Side::Left));
        let backward_range = f.range(&backward);
        backward.release(&mut f.grid);

        assert_eq!(forward_range, backward_range, "{kind:?}");
        assert!(forward_range.is_some(), "{kind:?}");
    }
}

// ── `Block` ─────────────────────────────────────────────────────────────────

#[test]
fn block_normalises_columns_not_rows() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst\r\nuvwxyz0123");

    // Dragged up and to the left: the rows order, the columns swap.
    let selection = f.select(
        SelectionKind::Block,
        (f.at(2, 7), Side::Left),
        (f.at(0, 3), Side::Right),
    );
    let range = f.range(&selection).expect("a block range");
    assert!(range.is_block);
    assert_eq!(range.start.row, f.row_id(0));
    assert_eq!(range.end.row, f.row_id(2));
    assert_eq!((range.start.col, range.end.col), (4, 6));
    selection.release(&mut f.grid);

    // Dragged down and to the left: the same rectangle.
    let mirrored = f.select(
        SelectionKind::Block,
        (f.at(0, 7), Side::Left),
        (f.at(2, 3), Side::Right),
    );
    assert_eq!(f.range(&mirrored), Some(range));
}

/// The one-column block whose end cannot step left: the reference builds an
/// inverted rectangle here, which its own `SelectionRange::new` would assert on.
#[test]
fn block_that_would_invert_its_columns_is_empty() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst");

    let selection = f.select(
        SelectionKind::Block,
        (f.at(0, 0), Side::Right),
        (f.at(1, 0), Side::Left),
    );
    assert_eq!(f.range(&selection), None);
}

#[test]
fn block_is_empty_by_the_column_rule_alone() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst\r\nuvwxyz0123");

    // Same column, same side, three rows apart.
    let selection = f.select(
        SelectionKind::Block,
        (f.at(0, 4), Side::Left),
        (f.at(2, 4), Side::Left),
    );
    assert_eq!(f.range(&selection), None);
    selection.release(&mut f.grid);

    // Adjacent columns, right -> left, on different rows.
    let selection = f.select(
        SelectionKind::Block,
        (f.at(0, 4), Side::Right),
        (f.at(2, 5), Side::Left),
    );
    assert_eq!(f.range(&selection), None);
}

#[test]
fn block_text_extracts_a_rectangle() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst\r\nuvwxyz0123");

    let selection = f.span(SelectionKind::Block, f.at(0, 2), f.at(2, 4));
    assert_eq!(f.text(&selection).as_deref(), Some("cde\nmno\nwxy"));
}

#[test]
fn block_text_trims_each_row_on_its_own() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefgh\r\nkl\r\nuvwxyz");

    let selection = f.span(SelectionKind::Block, f.at(0, 1), f.at(2, 6));
    assert_eq!(f.text(&selection).as_deref(), Some("bcdefg\nl\nvwxyz"));
}

#[test]
fn block_with_wide_chars_at_the_edges() {
    let mut f = fixture(5, 10, 100);
    // Row 0: 中(0) spacer(1) 文(2) spacer(3) a b c
    // Row 1: x y 中(2) spacer(3) z z
    f.feed("中文abc\r\nxy中zz");

    // The left edge lands on a spacer and pulls its glyph in; the right edge
    // clips a spacer and keeps its glyph.
    let selection = f.span(SelectionKind::Block, f.at(0, 1), f.at(1, 3));
    assert_eq!(f.text(&selection).as_deref(), Some("中文\ny中"));
}

// ── `Semantic` ──────────────────────────────────────────────────────────────

#[test]
fn semantic_expansion_uses_the_escape_chars() {
    let mut f = fixture(5, 20, 100);
    f.feed("foo bar-baz qux");

    let pos = f.at(0, 5);
    let selection = f.select(
        SelectionKind::Semantic,
        (pos, Side::Left),
        (pos, Side::Left),
    );
    // The default set has no '-', so the hyphenated word is one word.
    assert_eq!(f.text(&selection).as_deref(), Some("bar-baz"));
    // Adding '-' to the set splits it, and nothing else changed.
    assert_eq!(f.text_with(&selection, " -").as_deref(), Some("bar"));
}

#[test]
fn semantic_stops_at_an_unwrapped_row_boundary() {
    let mut f = fixture(5, 10, 100);
    f.feed("abc\r\ndef");

    let pos = f.at(1, 1);
    let selection = f.select(
        SelectionKind::Semantic,
        (pos, Side::Left),
        (pos, Side::Left),
    );
    assert_eq!(f.text(&selection).as_deref(), Some("def"));
    assert_eq!(f.range(&selection).unwrap().start, f.at(1, 0));
}

#[test]
fn semantic_runs_across_a_wrapped_row() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghijklm");

    let pos = f.at(1, 0);
    let selection = f.select(
        SelectionKind::Semantic,
        (pos, Side::Left),
        (pos, Side::Left),
    );
    assert_eq!(f.text(&selection).as_deref(), Some("abcdefghijklm"));
}

#[test]
fn semantic_expands_over_wide_chars_without_splitting_a_pair() {
    let mut f = fixture(5, 10, 100);
    f.feed("中文 abc");

    let pos = f.at(0, 0);
    let selection = f.select(
        SelectionKind::Semantic,
        (pos, Side::Left),
        (pos, Side::Left),
    );
    assert_eq!(f.text(&selection).as_deref(), Some("中文"));
}

#[test]
fn semantic_bracket_match_when_the_anchors_coincide() {
    let mut f = fixture(5, 20, 100);
    f.feed("foo(bar(baz))");

    let opening = f.at(0, 3);
    let forwards = f.select(
        SelectionKind::Semantic,
        (opening, Side::Left),
        (opening, Side::Left),
    );
    assert_eq!(f.text(&forwards).as_deref(), Some("(bar(baz))"));
    let expected = f.range(&forwards);
    forwards.release(&mut f.grid);

    // The same pair, found from the closing bracket.
    let closing = f.at(0, 12);
    let backwards = f.select(
        SelectionKind::Semantic,
        (closing, Side::Left),
        (closing, Side::Left),
    );
    assert_eq!(f.range(&backwards), expected);
}

#[test]
fn semantic_is_never_empty() {
    let mut f = fixture(5, 10, 100);
    f.feed("word");

    let pos = f.at(0, 1);
    let selection = f.select(
        SelectionKind::Semantic,
        (pos, Side::Right),
        (pos, Side::Left),
    );
    assert!(f.range(&selection).is_some());
}

// ── `Lines` ─────────────────────────────────────────────────────────────────

#[test]
fn lines_expands_across_wrapped_continuations() {
    let mut f = fixture(5, 10, 100);
    f.feed("0123456789abcdefghijklmno");

    let pos = f.at(1, 3);
    let selection = f.select(SelectionKind::Lines, (pos, Side::Left), (pos, Side::Left));
    let range = f.range(&selection).expect("a lines range");
    assert_eq!(range.start, f.at(0, 0));
    assert_eq!(range.end, f.at(2, 9));
    assert_eq!(
        f.text(&selection).as_deref(),
        Some("0123456789abcdefghijklmno\n")
    );
}

#[test]
fn lines_is_never_empty() {
    let mut f = fixture(5, 10, 100);
    f.feed("hi");

    let pos = f.at(0, 4);
    let selection = f.select(SelectionKind::Lines, (pos, Side::Right), (pos, Side::Left));
    assert_eq!(f.text(&selection).as_deref(), Some("hi\n"));
}

// ── Text extraction ─────────────────────────────────────────────────────────

#[test]
fn text_joins_wrapped_rows_without_a_newline() {
    let mut f = fixture(5, 10, 100);
    f.feed("0123456789abcde");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(1, 4));
    assert_eq!(f.text(&selection).as_deref(), Some("0123456789abcde"));
}

#[test]
fn text_puts_a_newline_between_unwrapped_rows_and_trims_trailing_blanks() {
    let mut f = fixture(5, 10, 100);
    f.feed("hi\r\nthere");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(1, 9));
    assert_eq!(f.text(&selection).as_deref(), Some("hi\nthere"));
}

#[test]
fn text_skips_wide_spacers_and_emits_whole_graphemes() {
    let mut f = fixture(5, 10, 100);
    f.feed("a中b");
    f.feed("e\u{301}");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 4));
    assert_eq!(f.text(&selection).as_deref(), Some("a中be\u{301}"));
    selection.release(&mut f.grid);

    // Starting on a spacer covers the glyph it belongs to, once.
    let from_spacer = f.span(SelectionKind::Simple, f.at(0, 2), f.at(0, 3));
    assert_eq!(f.text(&from_spacer).as_deref(), Some("中b"));
}

#[test]
fn text_includes_a_wide_glyph_whose_leading_spacer_ends_the_selection() {
    let mut f = fixture(5, 10, 100);
    // Nine narrow cells leave one column, too narrow for the wide glyph, which
    // wraps and leaves a `LeadingWideSpacer` behind.
    f.feed("abcdefghi中");
    assert_eq!(
        f.grid.screen().row(f.row_id(0)).cell(9).width(),
        crate::cell::CellWidth::LeadingWideSpacer
    );

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 9));
    assert_eq!(f.text(&selection).as_deref(), Some("abcdefghi中"));
}

#[test]
fn text_emits_tab_cells_as_tabs() {
    let mut f = fixture(5, 20, 100);
    f.feed("a\tb");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 8));
    assert_eq!(f.text(&selection).as_deref(), Some("a\tb"));
}

#[test]
fn text_of_an_empty_selection_is_empty() {
    let mut f = fixture(5, 10, 100);
    f.feed("abc");

    let pos = f.at(0, 1);
    let empty = f.select(SelectionKind::Simple, (pos, Side::Left), (pos, Side::Left));
    assert_eq!(f.text(&empty), None);
    empty.release(&mut f.grid);

    // A range that covers only blanks resolves, and yields nothing.
    let blanks = f.span(SelectionKind::Simple, f.at(1, 0), f.at(1, 3));
    assert_eq!(f.text(&blanks).as_deref(), Some(""));
}

#[test]
fn select_all_covers_history_and_the_viewport() {
    let mut f = fixture(3, 10, 100);
    for index in 0..8 {
        f.feed(&format!("line{index}\r\n"));
    }
    f.feed("tail");

    let selection = Selection::all(&mut f.grid);
    let text = f.text(&selection).expect("select_all is never empty");
    assert!(text.starts_with("line0\n"), "{text:?}");
    assert!(text.ends_with("tail"), "{text:?}");
    for index in 0..8 {
        assert!(text.contains(&format!("line{index}")), "{text:?}");
    }
}

// ── Membership ──────────────────────────────────────────────────────────────

/// The LLD's verification list calls this
/// `contains_cell_extends_a_wide_char_to_its_spacer`, which states the rule
/// backwards: the reference asks whether a `Wide` cell's **trailing spacer** is
/// selected, so the pull runs from the spacer to the glyph. Renamed to match the
/// behaviour; the LLD sentence is the design owner's to correct.
#[test]
fn contains_cell_pulls_in_the_wide_partner_of_a_selected_spacer() {
    let mut f = fixture(5, 10, 100);
    f.feed("a中b");

    // The range starts on the spacer; the glyph to its left still paints.
    let selection = f.span(SelectionKind::Simple, f.at(0, 2), f.at(0, 3));
    let range = f.range(&selection).expect("a range");
    let screen = f.grid.screen();
    assert!(!range.contains(f.at(0, 1)));
    assert!(range.contains_cell(screen, f.at(0, 1), None));
    assert!(!range.contains_cell(screen, f.at(0, 0), None));
    selection.release(&mut f.grid);

    // And not the other way: selecting only the glyph leaves its spacer alone.
    let glyph_only = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 1));
    let range = f.range(&glyph_only).expect("a range");
    let screen = f.grid.screen();
    assert!(!range.contains_cell(screen, f.at(0, 2), None));
}

#[test]
fn a_block_cursor_on_a_corner_is_not_inverted() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij");

    let selection = f.span(SelectionKind::Simple, f.at(0, 2), f.at(0, 5));
    let range = f.range(&selection).expect("a range");
    let screen = f.grid.screen();
    let corner = f.at(0, 2);
    assert!(range.contains_cell(screen, corner, None));
    assert!(!range.contains_cell(screen, corner, Some(corner)));
    // Only the corners: a block cursor in the middle still inverts.
    let middle = f.at(0, 3);
    assert!(range.contains_cell(screen, middle, Some(middle)));
}

// ── Rotation through the anchor list ────────────────────────────────────────

#[test]
fn anchors_follow_a_region_scroll() {
    let mut f = fixture(5, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc\r\nddd");

    let selection = f.span(SelectionKind::Simple, f.at(2, 0), f.at(2, 2));
    assert_eq!(f.text(&selection).as_deref(), Some("ccc"));

    // The reference rotates a viewport-relative range; here the content carries
    // its anchors, so a region scroll simply moves the selection with it.
    f.grid.scroll_up(ScrollRegion { top: 1, bottom: 4 }, 1);
    assert_eq!(f.text(&selection).as_deref(), Some("ccc"));
    assert_eq!(f.range(&selection).unwrap().start, f.at(1, 0));
}

#[test]
fn insert_lines_inside_a_region_moves_the_selection() {
    let mut f = fixture(5, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc\r\nddd");

    let selection = f.span(SelectionKind::Simple, f.at(1, 0), f.at(1, 2));
    f.goto(1, 0);
    f.grid.insert_lines(1);

    assert_eq!(f.text(&selection).as_deref(), Some("bbb"));
    assert_eq!(f.range(&selection).unwrap().start, f.at(2, 0));
}

#[test]
fn delete_lines_inside_a_region_moves_the_selection_and_kills_a_deleted_one() {
    let mut f = fixture(5, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc\r\nddd");

    let moved = f.span(SelectionKind::Simple, f.at(2, 0), f.at(2, 2));
    f.goto(1, 0);
    f.grid.delete_lines(1);
    assert_eq!(f.text(&moved).as_deref(), Some("ccc"));
    assert_eq!(f.range(&moved).unwrap().start, f.at(1, 0));

    // A selection on the row `DL` discards loses its content instead.
    let mut f = fixture(5, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc\r\nddd");
    let deleted = f.span(SelectionKind::Simple, f.at(1, 0), f.at(1, 2));
    f.goto(1, 0);
    f.grid.delete_lines(1);
    assert_eq!(f.range(&deleted), None);
}

/// Correction C15, for the design owner to confirm.
///
/// A region scroll discards the rows at the region top. The reference keeps the
/// selection alive and clamps the endpoint that was discarded to
/// `(range_top, column 0, Side::Left)`
/// (`vendor/alacritty_terminal/src/selection.rs:160-166`, pinned by its own
/// `rotate_in_region_up`). Here the anchor dies with its content and the whole
/// selection resolves to `None`.
///
/// Deliberate: the clamp leaves the selection covering text the user never
/// selected, which is exactly what `selection.md` says not to do — "cleared
/// rather than pointing at unrelated text". Declared because it is a
/// user-visible change from the reference either way.
#[test]
fn an_endpoint_scrolled_out_of_a_region_top_is_killed_not_clamped() {
    let mut f = fixture(5, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc\r\nddd");

    // The start sits on the row the scroll is about to discard; the end does not.
    let selection = f.span(SelectionKind::Simple, f.at(1, 0), f.at(3, 2));
    assert!(f.range(&selection).is_some());

    f.grid.scroll_up(ScrollRegion { top: 1, bottom: 4 }, 1);

    // The reference would report `(row 1, col 0)..=(row 2, col 2)`.
    assert_eq!(f.range(&selection), None);
    assert_eq!(f.text(&selection), None);
}

#[test]
fn a_selection_survives_a_scroll_into_history() {
    let mut f = fixture(3, 10, 100);
    f.feed("aaa\r\nbbb\r\nccc");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 2));
    for _ in 0..5 {
        f.feed("\r\nmore");
    }

    // Five rows above the screen top, still on its own content.
    assert_eq!(f.text(&selection).as_deref(), Some("aaa"));
    assert_eq!(f.range(&selection).unwrap().start, f.at(-5, 0));
}

#[test]
fn selection_clears_when_an_anchor_is_trimmed() {
    let mut f = fixture(3, 10, 100);
    f.grid.set_scrollback_limit(2);
    f.feed("aaa\r\nbbb\r\nccc");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 2));
    assert!(f.range(&selection).is_some());

    for _ in 0..4 {
        f.feed("\r\nmore");
    }
    assert_eq!(f.range(&selection), None);
    assert_eq!(f.text(&selection), None);
}

#[test]
fn viewport_scrolling_does_not_change_the_range() {
    let mut f = fixture(3, 10, 100);
    for index in 0..8 {
        f.feed(&format!("line{index}\r\n"));
    }

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 4));
    let before = f.range(&selection);
    let text = f.text(&selection);

    f.grid.screen_mut().scroll_viewport(-3);
    assert_eq!(f.range(&selection), before);
    assert_eq!(f.text(&selection), text);

    f.grid.screen_mut().scroll_to_bottom();
    assert_eq!(f.range(&selection), before);
}

#[test]
fn selection_survives_a_row_only_resize_and_clears_on_a_column_resize() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij\r\nklmnopqrst");

    let selection = f.span(SelectionKind::Simple, f.at(0, 2), f.at(0, 6));
    assert_eq!(f.text(&selection).as_deref(), Some("cdefg"));

    // Rows only: the anchors are remapped and the selection is kept.
    f.grid
        .resize(Size { rows: 8, cols: 10 }, ResizePolicy::BottomAnchor);
    assert_eq!(f.text(&selection).as_deref(), Some("cdefg"));

    // Trap 28: a width change invalidates it.
    f.grid
        .resize(Size { rows: 8, cols: 14 }, ResizePolicy::BottomAnchor);
    assert_eq!(f.range(&selection), None);
}

#[test]
fn a_column_reflow_round_trip_carries_the_anchors_and_the_text() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghijklmnopqrstuvwxyz");

    let (start, end) = (f.at(0, 2), f.at(1, 4));
    let selection = f.span(SelectionKind::Simple, start, end);
    let before = f.text(&selection).expect("a selection");
    assert_eq!(before, "cdefghijklmno");

    // The same two positions, tracked by anchors the trap-28 kill does not
    // reach: `Anchors::kill_selection` matches on **kind**, not on lane or
    // position (`reflow-and-resize.md`), which is what this pins.
    let marks = {
        let anchors = f.grid.anchors_mut();
        (
            anchors.register(AnchorKind::Mark(0), start),
            anchors.register(AnchorKind::Mark(1), end),
        )
    };

    f.grid
        .resize(Size { rows: 5, cols: 6 }, ResizePolicy::BottomAnchor);
    f.grid
        .resize(Size { rows: 5, cols: 10 }, ResizePolicy::BottomAnchor);

    // The selection itself is gone, by the matrix.
    assert_eq!(f.range(&selection), None);
    // The marks came through the same `Anchors::remap`, on their characters.
    let moved = {
        let anchors = f.grid.anchors();
        (
            anchors.get(marks.0).expect("the start survived"),
            anchors.get(marks.1).expect("the end survived"),
        )
    };
    let replayed = f.span(SelectionKind::Simple, moved.0, moved.1);
    assert_eq!(f.text(&replayed).as_deref(), Some(before.as_str()));
}

// ── The invalidation matrix ─────────────────────────────────────────────────

#[test]
fn invalidation_matrix() {
    // The selection sits on screen rows 1..=2 throughout; only the cursor row
    // and the operation change.
    let cases = [
        (Invalidation::EraseLine, 3, false),
        (Invalidation::EraseLine, 2, true),
        (Invalidation::EraseBelow, 3, false),
        (Invalidation::EraseBelow, 1, true),
        (Invalidation::EraseAbove, 0, false),
        (Invalidation::EraseAbove, 3, true),
        (Invalidation::EraseScreen, 0, true),
        (Invalidation::EraseHistory, 0, false),
        (Invalidation::SwapAlt, 0, true),
        (Invalidation::Reset, 0, true),
    ];
    for (op, cursor_row, expected) in cases {
        let mut f = fixture(5, 10, 100);
        f.feed("aaa\r\nbbb\r\nccc\r\nddd\r\neee");
        let selection = f.span(SelectionKind::Simple, f.at(1, 0), f.at(2, 2));
        f.goto(cursor_row, 0);
        assert_eq!(
            selection.invalidated_by(&f.grid, op),
            expected,
            "{op:?} with the cursor on row {cursor_row}"
        );
    }

    // `ED 3` clears a selection that reaches into history, and only that one.
    let mut f = fixture(3, 10, 100);
    for index in 0..6 {
        f.feed(&format!("line{index}\r\n"));
    }
    let in_history = f.span(SelectionKind::Simple, f.at(-2, 0), f.at(-2, 4));
    assert!(in_history.invalidated_by(&f.grid, Invalidation::EraseHistory));
    // And only `ED 3` reaches it: `ED 1` erases the screen top down to the
    // cursor, which is the whole screen here and still none of history.
    f.goto(2, 0);
    assert!(!in_history.invalidated_by(&f.grid, Invalidation::EraseAbove));
    assert!(!in_history.invalidated_by(&f.grid, Invalidation::EraseBelow));
    assert!(!in_history.invalidated_by(&f.grid, Invalidation::EraseLine));
    in_history.release(&mut f.grid);

    let on_screen = f.span(SelectionKind::Simple, f.at(1, 0), f.at(1, 4));
    assert!(!on_screen.invalidated_by(&f.grid, Invalidation::EraseHistory));

    // A selection whose content is gone is invalid under every operation.
    // `kill_selection` is kind-scoped: it reaches every selection anchor and
    // nothing else (`reflow-and-resize.md`).
    f.grid.anchors_mut().kill_selection();
    assert!(on_screen.invalidated_by(&f.grid, Invalidation::EraseLine));
    assert_eq!(f.range(&on_screen), None);
}

// ── Hit testing ─────────────────────────────────────────────────────────────

#[test]
fn hit_test_side_and_clamping() {
    let mut f = fixture(5, 10, 100);
    for index in 0..8 {
        f.feed(&format!("line{index}\r\n"));
    }

    assert_eq!(hit_test(&f.grid, 1.0, 3.2), (f.at(1, 3), Side::Left));
    assert_eq!(hit_test(&f.grid, 1.0, 3.7), (f.at(1, 3), Side::Right));
    assert_eq!(hit_test(&f.grid, 1.9, 0.5), (f.at(1, 0), Side::Right));

    // Out of range clamps rather than leaving the live rows.
    assert_eq!(hit_test(&f.grid, -4.0, -2.0), (f.at(0, 0), Side::Left));
    assert_eq!(hit_test(&f.grid, 99.0, 99.0), (f.at(4, 9), Side::Left));
    assert_eq!(
        hit_test(&f.grid, f32::NAN, f32::NAN),
        (f.at(0, 0), Side::Left)
    );

    // Scrolled back, row 0 is the first *visible* row, not the screen top.
    f.grid.screen_mut().scroll_viewport(-2);
    assert_eq!(hit_test(&f.grid, 0.0, 0.0), (f.at(-2, 0), Side::Left));
}

// ── Anchor hygiene and cost ─────────────────────────────────────────────────

#[test]
fn anchors_are_released_on_clear() {
    let mut f = fixture(5, 10, 100);
    f.feed("abcdefghij");

    let base = f.grid.anchors().live();
    let entries = f.grid.anchors().iter().count();

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 3));
    assert_eq!(f.grid.anchors().live(), base + 2);

    selection.release(&mut f.grid);
    assert_eq!(f.grid.anchors().live(), base);

    // The released slots are reused, so a drag cannot grow the list.
    let next = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 3));
    assert_eq!(f.grid.anchors().live(), base + 2);
    assert_eq!(f.grid.anchors().iter().count(), entries + 2);
    next.release(&mut f.grid);
}

#[test]
fn range_is_o1_and_does_not_allocate() {
    let mut f = fixture(5, 10, 10_000);
    for index in 0..2_000 {
        f.feed(&format!("line{index}\r\n"));
    }

    let selection = f.span(SelectionKind::Simple, f.at(-1_500, 0), f.at(4, 9));
    let before = f.grid.heap_bytes();
    let range = f.range(&selection);
    assert!(range.is_some());
    // A counting allocator needs `GlobalAlloc`, an `unsafe` trait this crate
    // does not have, so the grid's own capacities stand in: `to_range` reads
    // two anchors and two integers and touches no row.
    assert_eq!(f.grid.heap_bytes(), before);
    assert!(size_of::<Option<SelectionRange>>() <= 48);
}

#[test]
fn a_selection_reads_the_screen_its_anchors_are_on() {
    let mut f = fixture(5, 10, 100);
    f.feed("primary");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 6));
    f.grid.swap_alt();
    f.feed("alternate");

    // The alternate screen allocates from its own run of the id space, so the
    // selection still resolves against the primary rows it was made on — and
    // the matrix says the swap cleared it anyway.
    assert_eq!(f.text(&selection).as_deref(), Some("primary"));
    assert!(selection.invalidated_by(&f.grid, Invalidation::SwapAlt));
}

#[test]
fn a_cell_carrying_only_a_grapheme_id_still_reads_as_text() {
    let mut f = fixture(5, 10, 100);
    f.feed("a");
    f.feed("\u{0301}");

    let selection = f.span(SelectionKind::Simple, f.at(0, 0), f.at(0, 0));
    assert_eq!(f.text(&selection).as_deref(), Some("a\u{301}"));
    assert!(matches!(
        f.grid.screen().row(f.row_id(0)).cell(0).content(),
        CellContent::Grapheme(_)
    ));
}
