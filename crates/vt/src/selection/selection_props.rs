//! `selection::props::` — the properties that must hold for every grid and
//! every pair of anchors.
//!
//! `to_range` is what the painter inverts cells from and what the clipboard
//! copies: an unordered or out-of-bounds range is a panic or a wrong paste, and
//! neither shows up in a fixed-grid test. The reference has a `SelectionRange`
//! constructor that asserts `start <= end` and four range builders that bypass
//! it, which is exactly the hole this closes.
//!
//! `VT_PROPTEST_CASES` raises the case count for an exhaustive run; the default
//! keeps the debug suite inside the R-28 budget.

use proptest::prelude::*;

use super::*;
use crate::cell::CellContent;
use crate::grid::{PrintMode, RowFlags, RowId, Screen, Size, TerminalGrid};
use crate::intern::Interner;

const ROWS: u16 = 5;
const COLS: u16 = 8;
const SCROLLBACK: u32 = 20;

/// Dense, narrow, never blank: the oracle below counts cells, so a trailing
/// blank the extraction policy trims would make it a reimplementation of the
/// policy rather than an independent check. The trim itself is pinned by
/// `text_puts_a_newline_between_unwrapped_rows_and_trims_trailing_blanks`.
const GLYPHS: [char; 6] = ['a', 'b', 'Z', '(', ')', '-'];

fn cases() -> u32 {
    std::env::var("VT_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(256)
}

fn config() -> ProptestConfig {
    ProptestConfig::with_cases(cases())
}

struct Fixture {
    grid: TerminalGrid,
    interner: Interner,
}

/// One glyph per cell, plus a wrap flag per row.
fn build(glyphs: &[usize], wraps: &[bool]) -> Fixture {
    let mut f = Fixture {
        grid: TerminalGrid::new(
            Size {
                rows: ROWS,
                cols: COLS,
            },
            SCROLLBACK,
        ),
        interner: Interner::default(),
    };
    f.grid.begin_batch();
    let top = f.grid.screen().screen_top();
    let mut index = 0;
    for row in 0..ROWS {
        f.grid.screen_mut().goto(row, 0);
        for _ in 0..COLS {
            let glyph = GLYPHS[glyphs[index % glyphs.len()] % GLYPHS.len()];
            index += 1;
            f.grid.print(glyph, PrintMode::default(), &mut f.interner);
        }
        let wrapped = wraps[row as usize % wraps.len()];
        f.grid
            .screen_mut()
            .row_mut(top + u64::from(row))
            .set_wrapped(wrapped);
    }
    // The last row cannot continue onto a row that does not exist.
    f.grid
        .screen_mut()
        .row_mut(top + u64::from(ROWS - 1))
        .set_wrapped(false);
    f
}

fn pos(screen: &Screen, row: u16, col: u16) -> Pos {
    Pos {
        row: screen.screen_top() + u64::from(row.min(ROWS - 1)),
        col: col.min(COLS - 1),
    }
}

fn side(right: bool) -> Side {
    if right { Side::Right } else { Side::Left }
}

fn kind(index: usize) -> SelectionKind {
    match index % 4 {
        0 => SelectionKind::Simple,
        1 => SelectionKind::Block,
        2 => SelectionKind::Semantic,
        _ => SelectionKind::Lines,
    }
}

/// What the covered cells spell, computed independently of `text.rs`.
///
/// Valid only on the dense grid above: every cell holds a narrow, non-blank
/// scalar, so nothing is trimmed, no spacer is skipped and no grapheme expands.
fn expected_text(screen: &Screen, k: SelectionKind, range: SelectionRange) -> String {
    let last = COLS - 1;
    let mut text = String::new();
    let mut row = range.start.row;
    loop {
        let ends_here = row == range.end.row;
        let (start_col, end_col) = if range.is_block {
            (range.start.col, range.end.col)
        } else {
            (
                if row == range.start.row {
                    range.start.col
                } else {
                    0
                },
                if ends_here { range.end.col } else { last },
            )
        };
        for col in start_col..=end_col {
            if let CellContent::Scalar(c) = screen.row(row).cell(col).content() {
                text.push(c);
            }
        }
        if ends_here {
            break;
        }
        // A block always breaks; a run only breaks where the row ends and does
        // not continue.
        if range.is_block || (end_col >= last && !screen.row(row).wrapped()) {
            text.push('\n');
        }
        row = row + 1;
    }
    if matches!(k, SelectionKind::Lines) {
        text.push('\n');
    }
    text
}

proptest! {
    #![proptest_config(config())]

    /// Ordered, inside the live rows, inside the column count — for every kind
    /// and every pair of anchors.
    #[test]
    fn to_range_is_ordered_and_within_bounds(
        glyphs in prop::collection::vec(0usize..GLYPHS.len(), 1..40),
        wraps in prop::collection::vec(any::<bool>(), 1..6),
        kind_index in 0usize..4,
        (start_row, start_col, start_right) in (0u16..ROWS, 0u16..COLS, any::<bool>()),
        (end_row, end_col, end_right) in (0u16..ROWS, 0u16..COLS, any::<bool>()),
    ) {
        let mut f = build(&glyphs, &wraps);
        let (start, end) = {
            let screen = f.grid.screen();
            (pos(screen, start_row, start_col), pos(screen, end_row, end_col))
        };
        let k = kind(kind_index);
        let mut selection = Selection::new(&mut f.grid, k, start, side(start_right));
        selection.update(&mut f.grid, end, side(end_right));

        let Some(range) = selection.to_range(&f.grid, SEMANTIC_ESCAPE_CHARS) else {
            // Only the two kinds that are allowed to be empty may decline.
            prop_assert!(matches!(k, SelectionKind::Simple | SelectionKind::Block));
            return Ok(());
        };

        prop_assert!(range.start <= range.end, "{range:?}");
        if range.is_block {
            prop_assert!(range.start.col <= range.end.col, "{range:?}");
        }
        let screen = f.grid.screen();
        prop_assert!(range.start.row >= screen.oldest(), "{range:?}");
        prop_assert!(range.end.row <= screen.newest(), "{range:?}");
        prop_assert!(range.start.col < COLS, "{range:?}");
        prop_assert!(range.end.col < COLS, "{range:?}");
    }

    /// The copied text is exactly the covered visible cells, plus the line
    /// breaks the policy inserts.
    #[test]
    fn text_covers_exactly_the_cells_in_the_range(
        glyphs in prop::collection::vec(0usize..GLYPHS.len(), 1..40),
        wraps in prop::collection::vec(any::<bool>(), 1..6),
        kind_index in 0usize..4,
        (start_row, start_col, start_right) in (0u16..ROWS, 0u16..COLS, any::<bool>()),
        (end_row, end_col, end_right) in (0u16..ROWS, 0u16..COLS, any::<bool>()),
    ) {
        let mut f = build(&glyphs, &wraps);
        let (start, end) = {
            let screen = f.grid.screen();
            (pos(screen, start_row, start_col), pos(screen, end_row, end_col))
        };
        let k = kind(kind_index);
        let mut selection = Selection::new(&mut f.grid, k, start, side(start_right));
        selection.update(&mut f.grid, end, side(end_right));

        let Some(range) = selection.to_range(&f.grid, SEMANTIC_ESCAPE_CHARS) else {
            prop_assert_eq!(selection.text(&f.grid, &f.interner, SEMANTIC_ESCAPE_CHARS), None);
            return Ok(());
        };
        let text = selection
            .text(&f.grid, &f.interner, SEMANTIC_ESCAPE_CHARS)
            .expect("a resolved range always has text");
        let expected = expected_text(f.grid.screen(), k, range);
        prop_assert_eq!(&text, &expected, "{:?} {:?}", k, range);
        prop_assert_eq!(text.chars().count(), expected.chars().count());
    }

    /// A row-moving primitive never leaves the range malformed.
    ///
    /// It deliberately does **not** assert the text is unchanged: a scroll whose
    /// region contains one endpoint and not the other moves that endpoint alone,
    /// so the selection legitimately covers more or fewer rows afterwards. The
    /// reference kills the selection in that case from inside its own
    /// `rotate`; the anchor list cannot see it, and the packet records the gap.
    #[test]
    fn a_region_scroll_leaves_the_range_well_formed(
        glyphs in prop::collection::vec(0usize..GLYPHS.len(), 1..40),
        top in 0u16..ROWS,
        height in 1u16..ROWS,
        n in 1u16..3,
        down in any::<bool>(),
        (start_row, start_col) in (0u16..ROWS, 0u16..COLS),
        (end_row, end_col) in (0u16..ROWS, 0u16..COLS),
    ) {
        let mut f = build(&glyphs, &[false]);
        let (start, end) = {
            let screen = f.grid.screen();
            (pos(screen, start_row, start_col), pos(screen, end_row, end_col))
        };
        let mut selection = Selection::new(&mut f.grid, SelectionKind::Simple, start, Side::Left);
        selection.update(&mut f.grid, end, Side::Right);

        let region = crate::grid::ScrollRegion {
            top,
            bottom: (top + height).min(ROWS),
        };
        if down {
            f.grid.scroll_down(region, n);
        } else {
            f.grid.scroll_up(region, n);
        }

        let Some(range) = selection.to_range(&f.grid, SEMANTIC_ESCAPE_CHARS) else {
            return Ok(());
        };
        prop_assert!(range.start <= range.end, "{range:?}");
        let screen = f.grid.screen();
        prop_assert!(range.start.row >= screen.oldest(), "{range:?}");
        prop_assert!(range.end.row <= screen.newest(), "{range:?}");
        prop_assert!(range.end.col < COLS, "{range:?}");
        // And it still materialises rather than panicking on a moved row.
        prop_assert!(
            selection
                .text(&f.grid, &f.interner, SEMANTIC_ESCAPE_CHARS)
                .is_some()
        );
    }
}

/// The dense grid the properties run on really is dense: nothing above is
/// vacuously true because every row came out blank.
#[test]
fn the_property_grid_is_dense() {
    let f = build(&[0, 1, 2, 3, 4, 5], &[true, false]);
    let screen = f.grid.screen();
    for row in 0..ROWS {
        let id = screen.screen_top() + u64::from(row);
        for col in 0..COLS {
            assert!(
                !matches!(screen.row(id).cell(col).content(), CellContent::Scalar(' ')),
                "row {row} column {col} is blank"
            );
        }
    }
    assert!(!screen.row(screen.newest()).flags().contains(RowFlags::WRAPPED));
}
