//! Materialising the selected text.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md`
//! section "`selection_text`"; reference
//! `vendor/alacritty_terminal/src/term/mod.rs:544-645`.
//!
//! One function, shared by copy, the clipboard policy path and
//! `Terminal::selection_text`.

use crate::cell::{CellContent, CellWidth};
use crate::grid::{RowId, RowRef, Screen};
use crate::intern::Interner;
use crate::selection::{SelectionKind, SelectionRange};

pub(super) fn selection_text(
    screen: &Screen,
    interner: &Interner,
    kind: SelectionKind,
    range: SelectionRange,
) -> String {
    match kind {
        SelectionKind::Block => block_text(screen, interner, range),
        // Whole lines are copied with their break, so pasting them runs them.
        SelectionKind::Lines => bounds_text(screen, interner, range) + "\n",
        SelectionKind::Simple | SelectionKind::Semantic => bounds_text(screen, interner, range),
    }
}

/// Row by row, each row's own span, with the trailing break dropped.
fn bounds_text(screen: &Screen, interner: &Interner, range: SelectionRange) -> String {
    let last = screen.cols().saturating_sub(1);
    let mut text = String::new();
    let mut row = range.start.row;
    loop {
        let ends_here = row == range.end.row;
        let start_col = if row == range.start.row {
            range.start.col
        } else {
            0
        };
        let end_col = if ends_here { range.end.col } else { last };
        row_text(
            screen, interner, row, start_col, end_col, ends_here, &mut text,
        );
        if ends_here {
            break;
        }
        row = row + 1;
    }
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

/// A rectangle: the same columns on every row, each row trimmed and terminated.
fn block_text(screen: &Screen, interner: &Interner, range: SelectionRange) -> String {
    let mut text = String::new();
    let mut row = range.start.row;
    loop {
        let ends_here = row == range.end.row;
        let mut line = String::new();
        row_text(
            screen,
            interner,
            row,
            range.start.col,
            range.end.col,
            ends_here,
            &mut line,
        );
        text.push_str(line.trim_end());
        if ends_here {
            break;
        }
        text.push('\n');
        row = row + 1;
    }
    text
}

/// One row's contribution.
///
/// `last_row` is whether this is the final row of the selection, which is the
/// only row that can adopt a wide glyph its place-holder pushed onto the next
/// row.
fn row_text(
    screen: &Screen,
    interner: &Interner,
    id: RowId,
    mut start_col: u16,
    end_col: u16,
    last_row: bool,
    out: &mut String,
) {
    let row = screen.row(id);
    let cols = screen.cols();
    let last = cols.saturating_sub(1);
    let length = line_length(row, cols).min(end_col.saturating_add(1));

    // A selection that starts on a spacer covers the glyph the spacer belongs
    // to, so the glyph is emitted rather than lost.
    if start_col > 0 && row.cell(start_col).width() == CellWidth::WideSpacer {
        start_col -= 1;
    }

    let mut tab_run = false;
    for col in start_col..length {
        let cell = row.cell(col);
        // A tab's blanks belong to the tab: skip them until the next tab stop
        // or the next real character, so a copied table keeps its tabs.
        if tab_run {
            if screen.tabs().is_stop(col) || !matches!(cell.content(), CellContent::Scalar(' ')) {
                tab_run = false;
            } else {
                continue;
            }
        }
        if matches!(cell.content(), CellContent::Scalar('\t')) {
            tab_run = true;
        }
        // A spacer carries no glyph of its own; its `Wide` partner emitted one.
        if cell.width().is_spacer() {
            continue;
        }
        match cell.content() {
            CellContent::Scalar(c) => out.push(c),
            CellContent::Grapheme(grapheme) => out.extend(interner.resolve_grapheme(grapheme)),
        }
    }

    // A hard break: the selection reached the end of a row that does not
    // continue. A wrapped continuation joins without one, which is what makes a
    // wrapped command line paste as one line.
    if end_col >= last && !row.wrapped() {
        out.push('\n');
    }

    // The glyph whose place-holder ends this row lives at the start of the
    // next one, and the next row is outside the selection. The reference
    // implements this rule against `line - 1`, the row *above*, which cannot be
    // where the glyph went; the rule as described is implemented here.
    if last_row
        && end_col >= last
        && id < screen.newest()
        && row.cell(last).width() == CellWidth::LeadingWideSpacer
    {
        if let CellContent::Scalar(c) = screen.row(id + 1).cell(0).content() {
            out.push(c);
        }
    }
}

/// Where a row's content ends, which is where trailing blanks start.
///
/// A wrapped row has no trailing blanks to trim: every column of it is part of
/// the logical line. This is the reference's `LineLength::line_length`.
fn line_length(row: RowRef<'_>, cols: u16) -> u16 {
    if row.wrapped() {
        return cols;
    }
    row.cells()
        .iter()
        .rposition(|cell| !matches!(cell.content(), CellContent::Scalar(' ')))
        .map_or(0, |index| index as u16 + 1)
}
