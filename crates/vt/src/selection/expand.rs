//! Expanding an anchor into a word, a bracket pair or a whole logical line.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/selection.md`
//! section "Semantic expansion"; reference
//! Alacritty's `alacritty_terminal/src/term/search.rs:465-620`.
//!
//! Every rule the reference phrases as "the cell at the last column carries
//! `WRAPLINE`" is `RowRef::wrapped()` here: `WRAPPED` is a **row** flag
//! (deviation G1, `grid-and-scrollback.md`), which is strictly better defined —
//! the reference reads a flag off a cell an `EL` can erase.

use crate::cell::CellContent;
use crate::grid::{Pos, Screen};
use crate::selection::SelectionRange;

const BRACKET_PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

/// The word under the anchors, or the bracket pair when they coincide.
pub(super) fn range_semantic(
    screen: &Screen,
    mut start: Pos,
    mut end: Pos,
    escape_chars: &str,
) -> SelectionRange {
    if start == end {
        if let Some(matching) = bracket_search(screen, start) {
            if matching < start {
                start = matching;
            } else {
                end = matching;
            }
            return SelectionRange {
                start,
                end,
                is_block: false,
            };
        }
    }
    SelectionRange {
        start: semantic_search_left(screen, start, escape_chars),
        end: semantic_search_right(screen, end, escape_chars),
        is_block: false,
    }
}

/// Whole logical lines: walk out across wrapped continuations, then snap to the
/// first and last columns.
pub(super) fn range_lines(screen: &Screen, start: Pos, end: Pos) -> SelectionRange {
    SelectionRange {
        start: line_search_left(screen, start),
        end: line_search_right(screen, end),
        is_block: false,
    }
}

fn line_search_left(screen: &Screen, mut pos: Pos) -> Pos {
    while pos.row > screen.oldest() && screen.row(pos.row - 1).wrapped() {
        pos.row = pos.row - 1;
    }
    Pos {
        row: pos.row,
        col: 0,
    }
}

fn line_search_right(screen: &Screen, mut pos: Pos) -> Pos {
    while pos.row < screen.newest() && screen.row(pos.row).wrapped() {
        pos.row = pos.row + 1;
    }
    Pos {
        row: pos.row,
        col: screen.cols().saturating_sub(1),
    }
}

fn semantic_search_left(screen: &Screen, pos: Pos, escape_chars: &str) -> Pos {
    match inline_search_left(screen, pos, escape_chars) {
        // Landed on an escape character, so the word starts one cell on — past
        // any spacer, which carries no glyph of its own.
        Ok(found) => {
            let mut cursor = found;
            while let Some(next) = next_pos(screen, cursor) {
                cursor = next;
                if !is_spacer(screen, cursor) {
                    return cursor;
                }
            }
            found
        }
        Err(edge) => edge,
    }
}

fn semantic_search_right(screen: &Screen, pos: Pos, escape_chars: &str) -> Pos {
    match inline_search_right(screen, pos, escape_chars) {
        Ok(found) => prev_pos(screen, found).unwrap_or(found),
        Err(edge) => edge,
    }
}

/// The nearest escape character to the left, or the furthest cell reached.
///
/// Stops at a row boundary whose row lacks `WRAPPED`, so a word never runs
/// across two unrelated lines.
fn inline_search_left(screen: &Screen, mut pos: Pos, escape_chars: &str) -> Result<Pos, Pos> {
    let last = screen.cols().saturating_sub(1);
    while let Some(candidate) = prev_pos(screen, pos) {
        if candidate.col == last && !screen.row(candidate.row).wrapped() {
            break;
        }
        pos = candidate;
        if !is_spacer(screen, pos) && escape_chars.contains(scalar_at(screen, pos)) {
            return Ok(pos);
        }
    }
    Err(pos)
}

/// The nearest escape character to the right, or the furthest cell reached.
fn inline_search_right(screen: &Screen, mut pos: Pos, escape_chars: &str) -> Result<Pos, Pos> {
    let last = screen.cols().saturating_sub(1);
    if pos.col == last && !screen.row(pos.row).wrapped() {
        return Err(pos);
    }
    while let Some(candidate) = next_pos(screen, pos) {
        pos = candidate;
        if !is_spacer(screen, pos) && escape_chars.contains(scalar_at(screen, pos)) {
            return Ok(pos);
        }
        if pos.col == last && !screen.row(pos.row).wrapped() {
            break;
        }
    }
    Err(pos)
}

/// The matching bracket, counting nested pairs of the same kind.
fn bracket_search(screen: &Screen, pos: Pos) -> Option<Pos> {
    let opening = scalar_at(screen, pos);
    let (forward, closing) = BRACKET_PAIRS.iter().find_map(|&(open, close)| {
        if open == opening {
            Some((true, close))
        } else if close == opening {
            Some((false, open))
        } else {
            None
        }
    })?;

    let mut nested = 0i32;
    let mut cursor = pos;
    loop {
        cursor = if forward {
            next_pos(screen, cursor)?
        } else {
            prev_pos(screen, cursor)?
        };
        let c = scalar_at(screen, cursor);
        if c == closing && nested == 0 {
            return Some(cursor);
        } else if c == opening {
            nested += 1;
        } else if c == closing {
            nested -= 1;
        }
    }
}

/// The next cell in stream order, or `None` past the newest live row.
fn next_pos(screen: &Screen, pos: Pos) -> Option<Pos> {
    if pos.col + 1 < screen.cols() {
        return Some(Pos {
            row: pos.row,
            col: pos.col + 1,
        });
    }
    if pos.row >= screen.newest() {
        return None;
    }
    Some(Pos {
        row: pos.row + 1,
        col: 0,
    })
}

/// The previous cell in stream order, or `None` before the oldest live row.
fn prev_pos(screen: &Screen, pos: Pos) -> Option<Pos> {
    if pos.col > 0 {
        return Some(Pos {
            row: pos.row,
            col: pos.col - 1,
        });
    }
    if pos.row <= screen.oldest() {
        return None;
    }
    Some(Pos {
        row: pos.row - 1,
        col: screen.cols().saturating_sub(1),
    })
}

fn is_spacer(screen: &Screen, pos: Pos) -> bool {
    screen.row(pos.row).cell(pos.col).width().is_spacer()
}

/// A cell's character, for matching against the escape set or a bracket.
///
/// A grapheme cluster is neither: the escape set and `BRACKET_PAIRS` hold only
/// lone scalars, so a cluster reads as `NUL`, which is in no set. That keeps
/// `to_range` off the interner, which is what makes it allocation-free
/// (PERF-14). The cost is that a bracket carrying a combining mark is not
/// matched, where the reference matches on the base scalar.
fn scalar_at(screen: &Screen, pos: Pos) -> char {
    match screen.row(pos.row).cell(pos.col).content() {
        CellContent::Scalar(c) => c,
        CellContent::Grapheme(_) => '\0',
    }
}
