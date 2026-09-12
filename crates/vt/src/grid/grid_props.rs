//! `grid::props::` — the design's one grid property: any sequence of scrolls,
//! erases, inserts and deletes, with anchors registered throughout, leaves the
//! grid intact.
//!
//! The full two-screen integrity walk runs after **every** step, which is the
//! tier `testing-and-bench.md` reserves for property tests.

use proptest::prelude::*;

use super::*;
use crate::cell::Cell;
use crate::intern::Interner;

/// Deliberately small, so a sequence of twenty operations reaches every edge:
/// the region bottom, the last column, a full history.
const ROWS: u16 = 6;
const COLS: u16 = 8;
const SCROLLBACK: u32 = 12;

/// A wide glyph, a zero-width mark and plain text, because the print path's
/// hardest invariant is "no wide pair is ever split".
const GLYPHS: [char; 5] = ['a', ' ', '\u{ff21}', '\u{0301}', 'Z'];

#[derive(Debug, Clone, Copy)]
enum Op {
    /// Glyph, insert mode, autowrap, and the column to print it at — the column
    /// matters because the wide-pair invariants only break at specific ones.
    PrintAt(usize, bool, bool, u16),
    Print(usize, bool, bool),
    Linefeed,
    ReverseIndex,
    Wrapline,
    ScrollUp(u16, u16, u16),
    ScrollDown(u16, u16, u16),
    InsertLines(u16),
    DeleteLines(u16),
    EraseLine(u8),
    EraseChars(u16),
    DeleteChars(u16),
    InsertBlanks(u16),
    EraseDisplay(u8),
    Goto(u16, u16),
    SetRegion(u16, u16),
    Tab(u16),
    ScrollViewport(i32),
    SwapAlt,
    Mark(u16),
    ReleaseMark,
    Reset,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..GLYPHS.len(), any::<bool>(), any::<bool>())
            .prop_map(|(glyph, insert, autowrap)| Op::Print(glyph, insert, autowrap)),
        (0..GLYPHS.len(), any::<bool>(), any::<bool>(), 0..COLS)
            .prop_map(|(glyph, insert, autowrap, col)| Op::PrintAt(glyph, insert, autowrap, col)),
        Just(Op::Linefeed),
        Just(Op::ReverseIndex),
        Just(Op::Wrapline),
        (0..ROWS, 0..=ROWS, 0..ROWS + 2).prop_map(|(top, bottom, n)| Op::ScrollUp(top, bottom, n)),
        (0..ROWS, 0..=ROWS, 0..ROWS + 2)
            .prop_map(|(top, bottom, n)| Op::ScrollDown(top, bottom, n)),
        (0..ROWS + 2).prop_map(Op::InsertLines),
        (0..ROWS + 2).prop_map(Op::DeleteLines),
        (0u8..3).prop_map(Op::EraseLine),
        (0..COLS + 4).prop_map(Op::EraseChars),
        (0..COLS + 4).prop_map(Op::DeleteChars),
        (0..COLS + 4).prop_map(Op::InsertBlanks),
        (0u8..4).prop_map(Op::EraseDisplay),
        (0..ROWS, 0..COLS).prop_map(|(row, col)| Op::Goto(row, col)),
        (0..ROWS, 0..=ROWS).prop_map(|(top, bottom)| Op::SetRegion(top, bottom)),
        (0..3u16).prop_map(Op::Tab),
        (-4i32..4).prop_map(Op::ScrollViewport),
        Just(Op::SwapAlt),
        (0..ROWS).prop_map(Op::Mark),
        Just(Op::ReleaseMark),
        Just(Op::Reset),
    ]
}

/// `VT_PROPTEST_CASES` raises the case count for an exhaustive run, the same
/// knob `reflow::props` reads; the default keeps the debug suite inside the
/// R-28 budget.
fn config() -> ProptestConfig {
    let cases = std::env::var("VT_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(256);
    ProptestConfig::with_cases(cases)
}

/// Every live row of both screens, as a consumer reads it: the content, and the
/// stamp that is supposed to announce a change to it.
fn snapshot(grid: &TerminalGrid) -> Vec<(RowId, Vec<Cell>, bool, SeqNo)> {
    let mut out = Vec::new();
    for screen in [grid.primary(), grid.alt()] {
        let mut id = screen.oldest();
        while id <= screen.newest() {
            let row = screen.row(id);
            out.push((id, row.cells().to_vec(), row.wrapped(), row.seq()));
            id = id + 1;
        }
    }
    out
}

fn line_clear(mode: u8) -> LineClear {
    match mode {
        0 => LineClear::Right,
        1 => LineClear::Left,
        _ => LineClear::All,
    }
}

fn display_clear(mode: u8) -> DisplayClear {
    match mode {
        0 => DisplayClear::Below,
        1 => DisplayClear::Above,
        2 => DisplayClear::All,
        _ => DisplayClear::Saved,
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn scroll_and_erase_preserve_integrity(ops in prop::collection::vec(op(), 1..40)) {
        let mut grid = TerminalGrid::new(Size { rows: ROWS, cols: COLS }, SCROLLBACK);
        let mut interner = Interner::default();
        // Each mark remembers the lane it was registered on, so the walk below
        // can assert that work on one screen never kills the other's anchors.
        let mut marks: Vec<(AnchorId, bool)> = Vec::new();
        let mut tag = 0u32;

        for step in ops {
            let before = snapshot(&grid);
            let seq = grid.begin_batch();
            let active_alt = grid.alt_active();
            let inactive_alive: Vec<AnchorId> = marks
                .iter()
                .filter(|(id, alt)| *alt != active_alt && grid.anchors().get(*id).is_some())
                .map(|(id, _)| *id)
                .collect();
            match step {
                Op::Print(glyph, insert, autowrap) => {
                    grid.print(GLYPHS[glyph], PrintMode { insert, autowrap }, &mut interner);
                }
                Op::PrintAt(glyph, insert, autowrap, col) => {
                    let row = grid.screen().cursor_row_index();
                    grid.screen_mut().goto(row, col);
                    grid.print(GLYPHS[glyph], PrintMode { insert, autowrap }, &mut interner);
                }
                Op::Linefeed => { grid.linefeed(); }
                Op::ReverseIndex => { grid.reverse_index(); }
                Op::Wrapline => grid.wrapline(),
                Op::ScrollUp(top, bottom, n) => {
                    grid.scroll_up(ScrollRegion { top, bottom }, n);
                }
                Op::ScrollDown(top, bottom, n) => {
                    grid.scroll_down(ScrollRegion { top, bottom }, n);
                }
                Op::InsertLines(n) => { grid.insert_lines(n); }
                Op::DeleteLines(n) => { grid.delete_lines(n); }
                Op::EraseLine(mode) => grid.screen_mut().erase_line(line_clear(mode)),
                Op::EraseChars(n) => grid.screen_mut().erase_chars(n),
                Op::DeleteChars(n) => grid.screen_mut().delete_chars(n),
                Op::InsertBlanks(n) => grid.screen_mut().insert_blanks(n),
                Op::EraseDisplay(mode) => { grid.erase_display(display_clear(mode), &interner); }
                Op::Goto(row, col) => grid.screen_mut().goto(row, col),
                Op::SetRegion(top, bottom) => {
                    grid.screen_mut().set_region(top, bottom, CursorOrigin::Screen);
                }
                Op::Tab(count) => grid.put_tab(count),
                Op::ScrollViewport(delta) => grid.screen_mut().scroll_viewport(delta),
                Op::SwapAlt => grid.swap_alt(),
                Op::Mark(index) => {
                    let pos = Pos { row: grid.screen().row_of_index(index), col: 0 };
                    tag += 1;
                    let id = grid.anchors_mut().register(AnchorKind::Mark(tag), pos);
                    marks.push((id, grid.alt_active()));
                }
                Op::ReleaseMark => {
                    if let Some((id, _)) = marks.pop() {
                        grid.anchors_mut().release(id);
                    }
                }
                Op::Reset => grid.reset(),
            }
            grid.sync_anchors();
            grid.assert_integrity(Some(&interner));

            // The `US-0075` rework, stated as the property the whole damage
            // contract rests on: a row whose content changed carries this
            // batch's stamp and `DIRTY`. A de-allocated row that kept a stale
            // stamp — the shape of the original defect, where blanking dropped
            // the row header — fails here whatever produced it.
            let after = snapshot(&grid);
            for (id, cells, wrapped, was) in before {
                let Some((_, now, wraps, stamp)) =
                    after.iter().find(|(other, ..)| *other == id)
                else {
                    // Trimmed, or dropped off the bottom: the row left the live
                    // range, which `oldest` / `RowsTrimmed` reports instead.
                    continue;
                };
                if *now == cells && *wraps == wrapped {
                    continue;
                }
                prop_assert_eq!(
                    *stamp, seq,
                    "{:?} changed under {:?} but still reads the stamp {:?}",
                    id, step, was
                );
                prop_assert!(
                    grid.screen_of(id).row(id).flags().contains(RowFlags::DIRTY),
                    "{:?} changed under {:?} but is not DIRTY",
                    id,
                    step
                );
            }

            // B2's class: only `swap_alt`, `reset` and an explicit release touch
            // anchors on the inactive screen. Nothing else may.
            if !matches!(step, Op::SwapAlt | Op::Reset | Op::ReleaseMark) {
                for id in inactive_alive {
                    prop_assert!(
                        grid.anchors().get(id).is_some(),
                        "an operation on one screen killed an anchor on the other"
                    );
                }
            }
        }

        // The invariants the walk cannot express as a debug assertion.
        prop_assert!(grid.screen().scroll_offset() <= grid.screen().history_len());
        prop_assert!(grid.screen().history_len() <= SCROLLBACK);
        prop_assert!(grid.primary().row_range().end <= grid.alt().row_range().start);
        for (_, anchor) in grid.anchors().iter() {
            if !anchor.alive {
                continue;
            }
            let screen = grid.screen_of(anchor.pos.row);
            prop_assert!(anchor.pos.row >= screen.oldest() && anchor.pos.row <= screen.newest());
        }
    }
}
