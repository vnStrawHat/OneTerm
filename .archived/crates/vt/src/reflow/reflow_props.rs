//! `reflow::props::` — the seven properties the design names.
//!
//! Reflow is the component the research calls the most bug-prone in every
//! terminal (six open alacritty bugs, a guard in xterm.js commented "has been
//! known to fail for an unknown reason", a documented deadlock path in Windows
//! Terminal). None of those were found by a unit test, so this is where the
//! coverage goes.
//!
//! `VT_PROPTEST_CASES` raises the case count for an exhaustive run; the default
//! keeps the debug suite inside the R-28 budget.

use proptest::prelude::*;

use super::*;
use crate::cell::{Cell, CellContent, CellWidth};
use crate::grid::{AnchorId, AnchorKind, PrintMode, RowFlags, RowId, Screen, TerminalGrid};
use crate::intern::Interner;

const ROWS: u16 = 6;
const COLS: u16 = 10;
const SCROLLBACK: u32 = 60;

/// A wide glyph, a zero-width mark, a blank and plain text: the widths and the
/// trailing-blank trim are where reflow breaks.
const GLYPHS: [char; 6] = ['a', 'b', ' ', '\u{ff21}', '\u{0301}', 'Z'];

fn cases() -> u32 {
    std::env::var("VT_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(256)
}

fn config() -> ProptestConfig {
    ProptestConfig::with_cases(cases())
}

#[derive(Debug, Clone, Copy)]
enum Write {
    Glyph(usize),
    Newline,
    Home,
}

fn write() -> impl Strategy<Value = Write> {
    prop_oneof![
        8 => (0..GLYPHS.len()).prop_map(Write::Glyph),
        2 => Just(Write::Newline),
        1 => Just(Write::Home),
    ]
}

struct Fixture {
    grid: TerminalGrid,
    interner: Interner,
}

/// A grid with `writes` applied to the primary screen, and the same content on
/// the alternate screen so the "never reflows" property has something to watch.
fn build(writes: &[Write], on_alt: bool) -> Fixture {
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
    if on_alt {
        f.grid.swap_alt();
    }
    for step in writes {
        match step {
            Write::Glyph(index) => {
                f.grid
                    .print(GLYPHS[*index], PrintMode::default(), &mut f.interner);
            }
            Write::Newline => {
                f.grid.screen_mut().carriage_return();
                f.grid.linefeed();
            }
            Write::Home => f.grid.screen_mut().goto(0, 0),
        }
    }
    f
}

impl Fixture {
    fn screen(&self) -> &Screen {
        self.grid.screen()
    }

    /// Every logical line of a screen, trailing blanks trimmed. This is what a
    /// resize must preserve: the rows it is laid out on are not.
    fn logical_text(&self, screen: &Screen) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut id = screen.oldest();
        while id <= screen.newest() {
            screen.row_text(id, &self.interner.graphemes, &mut line);
            if !screen.row(id).wrapped() {
                lines.push(line.trim_end().to_owned());
                line.clear();
            }
            id = id + 1;
        }
        if !line.is_empty() {
            lines.push(line.trim_end().to_owned());
        }
        // How many blank rows trail the content is a function of the geometry —
        // a widen frees rows and the screen is padded back to its height — so
        // they are not content and are not compared.
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    }

    fn cell_at(&self, pos: Pos) -> Cell {
        self.screen().row(pos.row).cell(pos.col)
    }

    /// One anchor on every non-blank cell of the screen, up to `limit`.
    fn anchor_the_text(&mut self, limit: usize) -> Vec<(AnchorId, Cell)> {
        let mut marks = Vec::new();
        let (oldest, newest, cols) = {
            let screen = self.screen();
            (screen.oldest(), screen.newest(), screen.cols())
        };
        let mut id = oldest;
        let mut tag = 0;
        while id <= newest && marks.len() < limit {
            for col in 0..cols {
                let cell = self.screen().row(id).cell(col);
                if cell.is_blank() || marks.len() >= limit {
                    continue;
                }
                tag += 1;
                let anchor = self
                    .grid
                    .anchors_mut()
                    .register(AnchorKind::Mark(tag), Pos { row: id, col });
                marks.push((anchor, cell));
            }
            id = id + 1;
        }
        marks
    }
}

/// Every structural invariant reflow owes, checked on one screen.
fn assert_rows_are_well_formed(screen: &Screen) -> Result<(), TestCaseError> {
    let cols = screen.cols();
    let mut id = screen.oldest();
    while id <= screen.newest() {
        let row = screen.row(id);
        if row.is_allocated() {
            prop_assert_eq!(
                row.cells().len(),
                cols as usize,
                "a row is not the screen's width"
            );
            for (col, cell) in row.cells().iter().enumerate() {
                match cell.width() {
                    CellWidth::Wide => prop_assert!(
                        row.cells()
                            .get(col + 1)
                            .is_some_and(|next| next.width() == CellWidth::WideSpacer),
                        "a wide glyph lost its spacer"
                    ),
                    CellWidth::WideSpacer => prop_assert!(
                        col > 0 && row.cells()[col - 1].width() == CellWidth::Wide,
                        "a spacer lost its glyph"
                    ),
                    CellWidth::LeadingWideSpacer => prop_assert_eq!(
                        col + 1,
                        cols as usize,
                        "a leading spacer outside the last column"
                    ),
                    CellWidth::Narrow => {}
                }
            }
        }
        id = id + 1;
    }
    prop_assert!(
        !screen.row(screen.newest()).wrapped(),
        "the last row of the buffer continues a line that has nowhere to go"
    );
    prop_assert!(screen.scroll_offset() <= screen.history_len());
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// The text of every logical line survives a column round trip.
    #[test]
    fn text_survives_a_resize_round_trip(
        writes in prop::collection::vec(write(), 1..60),
        cols in 2u16..24,
    ) {
        let f0 = build(&writes, false);
        let before = f0.logical_text(f0.screen());

        let mut f = build(&writes, false);
        f.grid.resize(Size { rows: ROWS, cols }, ResizePolicy::BottomAnchor);
        f.grid.resize(Size { rows: ROWS, cols: COLS }, ResizePolicy::BottomAnchor);

        prop_assert_eq!(f.logical_text(f.screen()), before);
    }

    /// A tracked anchor is carried by the character it sat on.
    #[test]
    fn anchors_stay_on_their_character(
        writes in prop::collection::vec(write(), 1..60),
        cols in 2u16..24,
        rows in 2u16..10,
    ) {
        let mut f = build(&writes, false);
        let marks = f.anchor_the_text(24);
        f.grid.resize(Size { rows, cols }, ResizePolicy::BottomAnchor);

        for (anchor, cell) in marks {
            // A dead anchor is a legitimate answer: its logical line may have
            // been trimmed out of history (trap 32).
            let Some(pos) = f.grid.anchors().get(anchor) else {
                continue;
            };
            prop_assert_eq!(
                f.cell_at(pos).content(),
                cell.content(),
                "an anchor left its character"
            );
            prop_assert_eq!(f.cell_at(pos).width(), cell.width());
        }
    }

    /// No row is ever wider or narrower than the screen.
    #[test]
    fn no_row_exceeds_the_column_count(
        writes in prop::collection::vec(write(), 1..60),
        cols in 1u16..24,
        rows in 1u16..10,
    ) {
        let mut f = build(&writes, false);
        f.grid.resize(Size { rows, cols }, ResizePolicy::BottomAnchor);
        assert_rows_are_well_formed(f.screen())?;
        prop_assert_eq!(f.screen().cols(), cols);
        prop_assert_eq!(f.screen().rows(), rows);
    }

    /// Joining and splitting agree: the number of logical lines is the same at
    /// every width, and no line dangles off the end of the buffer.
    #[test]
    fn wrap_flags_are_consistent(
        writes in prop::collection::vec(write(), 1..40),
        cols in 2u16..24,
    ) {
        // Wide enough that nothing is trimmed, so line counts are comparable.
        let mut f = build(&writes, false);
        let before = f.logical_text(f.screen()).len();

        f.grid.resize(Size { rows: ROWS, cols }, ResizePolicy::BottomAnchor);
        assert_rows_are_well_formed(f.screen())?;
        if f.screen().history_len() < SCROLLBACK {
            prop_assert_eq!(f.logical_text(f.screen()).len(), before);
        }
    }

    /// Trap 33: a wide pair is never split across rows, at any width.
    #[test]
    fn wide_pairs_are_never_split(
        writes in prop::collection::vec(write(), 1..60),
        widths in prop::collection::vec(1u16..20, 1..6),
    ) {
        let mut f = build(&writes, false);
        for cols in widths {
            f.grid.resize(Size { rows: ROWS, cols }, ResizePolicy::BottomAnchor);
            assert_rows_are_well_formed(f.screen())?;
        }
    }

    /// Trap 29: the alternate screen is truncated and padded, never reflowed.
    #[test]
    fn alt_screen_never_reflows(
        writes in prop::collection::vec(write(), 1..60),
        cols in 1u16..24,
    ) {
        let mut f = build(&writes, true);
        let live = f.screen().newest().distance(f.screen().oldest());
        let keep = cols.min(COLS) as usize;
        let before: Vec<Vec<Cell>> = {
            let screen = f.screen();
            let mut rows = Vec::new();
            let mut id = screen.oldest();
            while id <= screen.newest() {
                rows.push(screen.row(id).cells()[..keep].to_vec());
                id = id + 1;
            }
            rows
        };

        f.grid.resize(Size { rows: ROWS, cols }, ResizePolicy::BottomAnchor);

        prop_assert!(f.grid.alt_active());
        let screen = f.grid.alt();
        prop_assert_eq!(
            screen.newest().distance(screen.oldest()),
            live,
            "the alternate screen gained or lost a row"
        );
        let mut id = screen.oldest();
        for row in before {
            // Only the cells a truncation could not have touched are compared;
            // a split pair is repaired at the new edge.
            let now = &screen.row(id).cells()[..keep];
            for (col, (cell, was)) in now.iter().zip(row.iter()).enumerate() {
                if col + 1 < keep && !was.width().is_spacer() && was.width() != CellWidth::Wide {
                    prop_assert_eq!(cell.content(), was.content(), "the alternate screen moved a cell");
                }
            }
            id = id + 1;
        }
        prop_assert_eq!(screen.cols(), cols);
    }

    /// The full two-screen walk holds after any sequence of resizes under either
    /// policy, with anchors registered throughout.
    #[test]
    fn integrity_holds_after_any_resize_sequence(
        writes in prop::collection::vec(write(), 1..40),
        sizes in prop::collection::vec((1u16..12, 1u16..20, any::<bool>()), 1..8),
        on_alt in any::<bool>(),
    ) {
        let mut f = build(&writes, on_alt);
        let marks = f.anchor_the_text(8);
        for (rows, cols, keep_top) in sizes {
            let policy = if keep_top {
                ResizePolicy::KeepViewportTop
            } else {
                ResizePolicy::BottomAnchor
            };
            f.grid.resize(Size { rows, cols }, policy);
            // `TerminalGrid::resize` runs the debug walk itself; these are the
            // invariants it cannot express as a debug assertion.
            f.grid.assert_integrity(Some(&f.interner));
            assert_rows_are_well_formed(f.grid.primary())?;
            assert_rows_are_well_formed(f.grid.alt())?;
            prop_assert!(f.grid.primary().history_len() <= SCROLLBACK);
            prop_assert_eq!(f.grid.alt().history_len(), 0);
            prop_assert!(f.grid.primary().row_range().end <= f.grid.alt().row_range().start);
            for (anchor, _) in &marks {
                if let Some(pos) = f.grid.anchors().get(*anchor) {
                    prop_assert!(pos.col < f.grid.screen_of(pos.row).cols());
                }
            }
        }
    }
}

/// Ids are allocated, never reused, and the ring never has to grow (R-30).
#[test]
fn reflow_allocates_fresh_ids_without_rehoming_the_ring() {
    let mut f = build(&[Write::Glyph(0); 40], false);
    let (mask, len) = (f.screen().ring_mask(), f.screen().ring_len());
    let mut newest = f.screen().newest();
    for cols in [4u16, 17, 3, 20] {
        f.grid
            .resize(Size { rows: ROWS, cols }, ResizePolicy::BottomAnchor);
        assert!(
            f.screen().oldest() > newest,
            "reflow must not reuse a live row id"
        );
        newest = f.screen().newest();
        assert_eq!(f.screen().ring_mask(), mask);
        assert_eq!(f.screen().ring_len(), len);
    }
}

/// The one thing `RowFlags` cannot say for itself after a reflow.
#[test]
fn reflowed_rows_carry_the_batch_stamp() {
    let mut f = build(&[Write::Glyph(0); 20], false);
    let seq = f.grid.begin_batch();
    f.grid.resize(
        Size {
            rows: ROWS,
            cols: 5,
        },
        ResizePolicy::BottomAnchor,
    );

    let screen = f.screen();
    let mut id = screen.oldest();
    while id <= screen.newest() {
        let row = screen.row(id);
        if row.is_allocated() {
            assert_eq!(row.seq(), seq, "row {id:?} was not stamped");
            assert!(row.flags().contains(RowFlags::DIRTY));
        }
        id = id + 1;
    }
}

/// A zero-width mark rides on the cell it attached to, across a reflow.
#[test]
fn grapheme_cells_survive_a_reflow() {
    let mut f = build(&[], false);
    for c in "ab\u{0301}cdefghijkl".chars() {
        f.grid.print(c, PrintMode::default(), &mut f.interner);
    }
    let before = f.logical_text(f.screen());

    f.grid.resize(
        Size {
            rows: ROWS,
            cols: 4,
        },
        ResizePolicy::BottomAnchor,
    );
    f.grid.resize(
        Size {
            rows: ROWS,
            cols: COLS,
        },
        ResizePolicy::BottomAnchor,
    );

    assert_eq!(f.logical_text(f.screen()), before);
    let screen = f.screen();
    let mut found = false;
    let mut id = screen.oldest();
    while id <= screen.newest() {
        for cell in screen.row(id).cells() {
            if matches!(cell.content(), CellContent::Grapheme(_)) {
                found = true;
            }
        }
        id = id + 1;
    }
    assert!(found, "the combining mark's cluster did not survive");
}

/// `RowId` arithmetic in the tests above assumes the primary lane.
const _: () = assert!(RowId::PRIMARY_ORIGIN.0 == 0);
