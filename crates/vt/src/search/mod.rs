//! Scrollback search: copy the grid text under your lock, match without it.
//!
//! Search is two phases on purpose. [`GridText::from_terminal`] copies one
//! `char` per cell for the whole grid — history and viewport — and is the only
//! phase that needs the terminal; [`search_grid_text`] then matches against
//! that copy, so a long scrollback search never holds the lock the byte pump
//! wants. The copy is deliberate: search is user-initiated and rare, unlike a
//! per-frame snapshot.
//!
//! A match names a **row identity** ([`RowId`]), not a coordinate, so it stays
//! correct when new output scrolls the grid underneath it. Turn one into a
//! viewport row with [`SearchMatch::display_row`].
//!
//! Matching is **character-based** (one `char` per grid cell), so a column
//! index in a [`SearchMatch`] is a cell column. The literal scanner's
//! case-insensitive mode uses ASCII case-folding
//! (`char::eq_ignore_ascii_case`): 1:1 per character, so column positions stay
//! exact, and it covers the dominant terminal use case (commands, logs and
//! paths are ASCII).
//!
//! ```
//! use oneterm_vt::search::{GridText, SearchOptions, SearchPattern, search_grid_text};
//! # use oneterm_vt::{Config, EventBatch, Size, Terminal};
//! # use std::time::Instant;
//! # let mut term = Terminal::new(Size { rows: 2, cols: 11 }, Config::default());
//! # term.feed(b"Hello World", &mut EventBatch::new(), Instant::now());
//! // Phase one, under the lock.
//! let text = GridText::from_terminal(&term);
//! // Phase two, without it.
//! let hits = search_grid_text(&text, SearchPattern::Literal("world"), SearchOptions::default());
//! assert_eq!(hits.len(), 1);
//! assert_eq!(hits[0].start_col, 6);
//! ```

use crate::cell::CellWidth;
use crate::grid::RowId;
use crate::terminal::Terminal;

mod literal;
#[cfg(feature = "regex")]
mod regex;

/// Search options.
///
/// `case_sensitive` defaults to `false` (the common expectation in terminal
/// search). `whole_word` requires word boundaries on both sides of the match.
///
/// Both apply to [`SearchPattern::Literal`] only. A regular expression owns its
/// own flags — `(?i)` and `\b` — so both fields are ignored for
/// [`SearchPattern::Regex`], and [`search_grid_text`] debug-asserts that they
/// are still at their defaults rather than letting them silently do nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct SearchOptions {
    /// Match case exactly. When `false` (default), ASCII letters are compared
    /// case-insensitively.
    pub case_sensitive: bool,
    /// Only match runs bounded by non-word characters (or line start/end).
    /// A "word char" is ASCII alphanumeric or `_`.
    pub whole_word: bool,
}

/// What to look for.
///
/// Neither form matches across a line boundary, wrapped continuations
/// included: each grid row is matched on its own.
///
/// The variant set depends on the `regex` feature, so the enum is
/// `#[non_exhaustive]` and a `match` on it always needs a wildcard arm — which
/// is what keeps the same code compiling whether or not the feature is on:
///
/// ```
/// use oneterm_vt::search::SearchPattern;
///
/// fn kind(pattern: SearchPattern<'_>) -> &'static str {
///     match pattern {
///         SearchPattern::Literal(_) => "literal",
///         _ => "something else",
///     }
/// }
///
/// assert_eq!(kind(SearchPattern::Literal("needle")), "literal");
/// ```
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum SearchPattern<'a> {
    /// A literal run of characters, matched cell by cell.
    Literal(&'a str),
    /// An already-compiled regular expression, matched against one row's
    /// characters at a time.
    ///
    /// The row is exactly as wide as the grid, so `^` and `$` anchor to the
    /// first and last **cell** of the row rather than to the end of the text
    /// somebody typed, and the blank cells to the right of the last glyph are
    /// spaces. Wide-character spacer cells read as `'\0'`.
    ///
    /// Compilation, and therefore any `RegexBuilder::size_limit`, is the
    /// caller's: this crate takes a reference to a finished `Regex`.
    #[cfg(feature = "regex")]
    #[cfg_attr(docsrs, doc(cfg(feature = "regex")))]
    Regex(&'a ::regex::Regex),
}

/// One search match, on the row that holds it.
///
/// `start_col`/`end_col` are column indices, 0-based, `end_col` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    /// The row the match sits on. Stable across scrolling and scrollback
    /// pushes, which is why it is published instead of a line number.
    pub row: RowId,
    /// First column of the match.
    pub start_col: usize,
    /// One past the last column of the match.
    pub end_col: usize,
}

impl SearchMatch {
    /// The match's row relative to `screen_top` — the viewport top at
    /// `display_offset == 0`. Negative is scrollback history (`-1` is the
    /// newest history row).
    #[inline]
    pub fn grid_line(&self, screen_top: RowId) -> i32 {
        (self.row.0 as i64 - screen_top.0 as i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
            as i32
    }

    /// Display row (0-based from the top of the viewport) for this match, given
    /// `screen_top` and the current scroll offset. May be negative or
    /// `>= num_lines` when the match is scrolled out of view — the caller
    /// should filter.
    #[inline]
    pub fn display_row(&self, screen_top: RowId, display_offset: usize) -> i32 {
        self.grid_line(screen_top)
            .saturating_add(display_offset as i32)
    }
}

/// One `char` per cell for the whole grid (scrollback history + viewport),
/// copied so the search itself can run without the embedder's lock.
///
/// Row identity is the engine's: rows are stored top-to-bottom starting at
/// [`GridText::oldest`], which is the [`RowId`] a [`SearchMatch`] carries. Each
/// row is exactly as wide as the grid, so the copy holds
/// [`rows`](GridText::rows) × columns `char`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridText {
    /// The topmost stored row (the oldest history row).
    oldest: RowId,
    /// Row stride.
    num_cols: usize,
    /// Row-major cell characters. Wide-char spacers are `'\0'`.
    chars: Vec<char>,
}

impl GridText {
    /// Copy the grid text of `term`, history and viewport.
    ///
    /// Call this under your lock; it is the only part of a search that needs
    /// the terminal. It allocates and writes one `char` — four bytes — per
    /// cell, so a 100 000-row history at 200 columns costs 20 million `char`s,
    /// which is 80 MB and takes as long as one pass over them.
    pub fn from_terminal(term: &Terminal) -> Self {
        let screen = term.screen();
        let graphemes = &term.interner().graphemes;
        let num_cols = usize::from(screen.cols());
        let range = screen.row_range();
        let rows = range.end.distance(range.start) as usize;
        let mut chars = Vec::with_capacity(rows * num_cols);
        for index in 0..rows {
            for cell in screen.row(range.start + index as u64).cells() {
                // Wide-char spacers carry no visible glyph — use a NUL placeholder so
                // they cannot be part of a match (the needle never contains NUL). This
                // keeps the column index aligned with the cell column.
                if cell.width() == CellWidth::WideSpacer {
                    chars.push('\0');
                } else {
                    chars.push(cell.text_char(graphemes));
                }
            }
        }
        Self {
            oldest: range.start,
            num_cols,
            chars,
        }
    }

    /// How many rows the copy holds.
    pub fn rows(&self) -> usize {
        self.chars.len().checked_div(self.num_cols).unwrap_or(0)
    }

    /// The `RowId` of the first stored row — the oldest history row.
    pub fn oldest(&self) -> RowId {
        self.oldest
    }

    /// The `RowId` of the stored row at `index`.
    fn row_id(&self, index: usize) -> RowId {
        self.oldest + index as u64
    }
}

/// Match `pattern` against a copied grid. Runs without any lock.
///
/// Returns matches in **top-to-bottom order** (oldest history first, newest
/// last) so forward navigation ("next") walks down the scrollback. Matches on
/// one row do not overlap, and no match spans a line boundary: the pattern is
/// matched against each row's cells, left to right.
///
/// An empty [`SearchPattern::Literal`] finds nothing. A regular expression that
/// can match the empty string finds one empty match per position, and
/// terminates.
pub fn search_grid_text(
    text: &GridText,
    pattern: SearchPattern<'_>,
    options: SearchOptions,
) -> Vec<SearchMatch> {
    if text.num_cols == 0 {
        return Vec::new();
    }
    match pattern {
        SearchPattern::Literal(query) => literal::search(text, query, options),
        #[cfg(feature = "regex")]
        SearchPattern::Regex(re) => regex::search(text, re, options),
    }
}

/// Snapshot `term` and search it for a literal in one step.
#[cfg(test)]
fn search_term(term: &Terminal, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
    search_grid_text(
        &GridText::from_terminal(term),
        SearchPattern::Literal(query),
        options,
    )
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
