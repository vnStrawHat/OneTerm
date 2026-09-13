//! Terminal scrollback search — framework-agnostic algorithm over a copied grid.
//!
//! The UI asks a `TerminalSession` for matches (`fn search`); the backend copies
//! the grid text under the engine lock ([`GridText::from_terminal`]) and matches
//! after releasing it ([`search_grid_text`]), so a long scrollback search never
//! stalls the pump (PERF-04). The copy is kept deliberately (R-19): search is
//! user-initiated and rare, unlike a per-frame snapshot. Matches are reported in
//! **grid coordinates**: negative values are scrollback history,
//! `0..num_lines-1` is the viewport at `display_offset = 0`.
//!
//! The UI converts a match to a display row with `display_row = line + display_offset`
//! (see `docs/terminal-backend.md` § coordinate systems) and scrolls the viewport
//! so the active match is visible.
//!
//! Matching is **character-based** (one `char` per grid cell). Case-insensitive
//! mode uses ASCII case-folding (`char::eq_ignore_ascii_case`) — this is 1:1 per
//! character so column positions stay exact, and covers the dominant terminal
//! use case (commands, logs, paths are ASCII).

use oneterm_vt::{CellWidth, RowId, Terminal};

/// Search options.
///
/// `case_sensitive` defaults to `false` (the common expectation in terminal
/// search). `whole_word` requires word boundaries on both sides of the match.
/// Regex is intentionally omitted for the MVP — the field set is extensible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    /// Match case exactly. When `false` (default), ASCII letters are compared
    /// case-insensitively.
    pub case_sensitive: bool,
    /// Only match runs bounded by non-word characters (or line start/end).
    /// A "word char" is ASCII alphanumeric or `_`.
    pub whole_word: bool,
}

/// One search match in **grid coordinates**.
///
/// The signed `line` and [`SearchMatch::display_row`] are the compatibility
/// surface: `crates/terminal-view/src/terminal_view/search.rs` both constructs
/// this struct and calls that method, so neither can become a `RowId` before
/// `US-0085` moves that file. Inside this crate the search is `RowId`-keyed
/// ([`GridText`]) and the conversion happens once, where a match is published.
///
/// `line` is the signed grid line:
/// - negative → scrollback history (`-1` = newest history line, just above the viewport top at `display_offset = 0`);
/// - `0..num_lines-1` → the viewport rows when `display_offset = 0`.
///
/// `start_col`/`end_col` are column indices, 0-based, `end_col` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub line: i32,
    pub start_col: usize,
    pub end_col: usize,
}

impl SearchMatch {
    /// Display row (0-based from the top of the viewport) for this match, given
    /// the current scroll offset. May be negative or `>= num_lines` when the
    /// match is scrolled out of view — the caller should filter.
    #[inline]
    pub fn display_row(&self, display_offset: usize) -> i32 {
        self.line + display_offset as i32
    }
}

/// One `char` per cell for the whole grid (scrollback history + viewport),
/// copied under the engine lock so the search itself can run without it.
///
/// Row identity is the engine's: rows are stored top-to-bottom starting at
/// [`GridText::oldest`], and the signed grid line a [`SearchMatch`] publishes is
/// derived from `screen_top` at the end. Each row is exactly `num_cols` chars,
/// so `chars.len() == rows × num_cols`.
///
/// The copy is deliberate (R-19): search is user-initiated and rare, unlike a
/// per-frame snapshot, and copying once under the lock is what keeps a long
/// scrollback search off the pump's back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GridText {
    /// The topmost stored row (the oldest history row).
    oldest: RowId,
    /// The row the reference calls `Line(0)`: the viewport top at
    /// `display_offset == 0`.
    screen_top: RowId,
    /// Row stride.
    num_cols: usize,
    /// Row-major cell characters. Wide-char spacers are `'\0'`.
    chars: Vec<char>,
}

impl GridText {
    /// Copy the grid text of `term`, history and viewport. O(rows×cols) chars;
    /// hold the lock only for this call.
    pub(crate) fn from_terminal(term: &Terminal) -> Self {
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
            screen_top: screen.screen_top(),
            num_cols,
            chars,
        }
    }

    /// The `RowId` of the stored row at `index`.
    fn row_id(&self, index: usize) -> RowId {
        self.oldest + index as u64
    }
}

/// Search a [`GridText`] snapshot for `query`.
///
/// Returns matches in **top-to-bottom order** (oldest history first, newest last)
/// so forward navigation ("next") walks down the scrollback.
///
/// Empty `query` → empty result. The query is matched against the per-line text
/// (cells joined left-to-right); matches do **not** span line boundaries.
pub(crate) fn search_grid_text(
    text: &GridText,
    query: &str,
    options: SearchOptions,
) -> Vec<SearchMatch> {
    if query.is_empty() || text.num_cols == 0 {
        return Vec::new();
    }

    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() || needle.len() > text.num_cols {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for (index, line_chars) in text.chars.chunks_exact(text.num_cols).enumerate() {
        // The one conversion out of `RowId`, at the point a match is published.
        let line = crate::engine_shim::row_to_line(text.row_id(index), text.screen_top);
        find_in_line(line_chars, &needle, line, options, &mut matches);
    }
    matches
}

/// Snapshot `term` and search it in one step (tests and single-shot callers).
#[cfg(test)]
pub(crate) fn search_term(
    term: &Terminal,
    query: &str,
    options: SearchOptions,
) -> Vec<SearchMatch> {
    search_grid_text(&GridText::from_terminal(term), query, options)
}

/// Find all (non-overlapping) occurrences of `needle` in a single line's char
/// buffer, appending matches for `line` to `out`.
fn find_in_line(
    line: &[char],
    needle: &[char],
    line_no: i32,
    options: SearchOptions,
    out: &mut Vec<SearchMatch>,
) {
    let n = needle.len();
    let len = line.len();
    if n == 0 || n > len {
        return;
    }

    let mut col = 0;
    while col + n <= len {
        if matches_at(line, col, needle, options.case_sensitive) {
            let ok_word = if options.whole_word {
                is_word_boundary(line, col) && is_word_boundary(line, col + n)
            } else {
                true
            };
            if ok_word {
                out.push(SearchMatch {
                    line: line_no,
                    start_col: col,
                    end_col: col + n,
                });
            }
            // Non-overlapping: advance past the match.
            col += n;
        } else {
            col += 1;
        }
    }
}

/// Compare `line[col..col+n]` with `needle` (ASCII case-folded when not
/// case-sensitive).
fn matches_at(line: &[char], col: usize, needle: &[char], case_sensitive: bool) -> bool {
    if case_sensitive {
        line[col..col + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a == b)
    } else {
        line[col..col + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }
}

/// `true` if the position `at` is a word boundary: the preceding char (if any)
/// is not a word char, or `at` is at the line start/end. "Word char" = ASCII
/// alphanumeric or `_`.
fn is_word_boundary(line: &[char], at: usize) -> bool {
    let before = if at == 0 { None } else { Some(line[at - 1]) };
    let after = if at >= line.len() {
        None
    } else {
        Some(line[at])
    };
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    match (before, after) {
        (None, _) | (_, None) => true,
        (Some(b), Some(a)) => !is_word(b) || !is_word(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GridSize;

    /// The `mock_term` contract the eleven tests below were written against:
    /// the grid is sized to the content — columns = the widest line by display
    /// width, rows = the line count — so nothing scrolls and row `0` is the
    /// first line. Lines are separated with `\r\n`, because none of these tests
    /// asks anything about a wrap flag.
    fn mock_term(text: &str) -> Terminal {
        let lines: Vec<&str> = text.split('\n').collect();
        let cols = lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|c| usize::from(oneterm_vt::scalar_width(c).unwrap_or(0)))
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .max(1);
        let mut term = crate::test_engine::terminal(GridSize {
            cols,
            lines: lines.len(),
        });
        crate::test_engine::feed(&mut term, lines.join("\r\n").as_bytes());
        term
    }

    #[test]
    fn empty_query_no_matches() {
        let term = mock_term("hello world");
        assert!(search_term(&term, "", SearchOptions::default()).is_empty());
    }

    #[test]
    fn single_match_case_insensitive_default() {
        let term = mock_term("Hello World");
        // Default (case_sensitive=false) → matches "Hello" and "World".
        let m = search_term(&term, "hello", SearchOptions::default());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].line, 0);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[0].end_col, 5);
    }

    #[test]
    fn case_sensitive_no_match_when_differs() {
        let term = mock_term("Hello World");
        let opts = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };
        assert!(search_term(&term, "hello", opts).is_empty());
        assert_eq!(search_term(&term, "Hello", opts).len(), 1);
    }

    #[test]
    fn multiple_matches_on_one_line_non_overlapping() {
        let term = mock_term("foo bar foo baz foo");
        // "foo bar foo baz foo" — three "foo".
        assert_eq!(term.screen().cols(), 19);
        let m = search_term(&term, "foo", SearchOptions::default());
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[1].start_col, 8);
        assert_eq!(m[2].start_col, 16);
        // Non-overlapping end cols.
        assert_eq!(m[0].end_col, 3);
    }

    #[test]
    fn multiple_lines_top_to_bottom() {
        let term = mock_term("alpha\nbeta\nalpha");
        let m = search_term(&term, "alpha", SearchOptions::default());
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].line, 0);
        assert_eq!(m[1].line, 2);
    }

    #[test]
    fn whole_word_excludes_substrings() {
        let term = mock_term("foo foobar foo");
        let opts = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        // "foobar" contains "foo" but is not bounded on the right → excluded.
        let m = search_term(&term, "foo", opts);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[1].start_col, 11);
    }

    #[test]
    fn whole_word_with_underscores() {
        // "foo_bar" — "foo" is a prefix of an identifier → not a whole word.
        let term = mock_term("foo_bar foo");
        let opts = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        let m = search_term(&term, "foo", opts);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_col, 8);
    }

    #[test]
    fn match_display_row_conversion() {
        let term = mock_term("hello");
        let m = search_term(&term, "hello", SearchOptions::default());
        assert_eq!(m.len(), 1);
        // No scrollback (display_offset = 0) → display row = line.
        assert_eq!(m[0].display_row(0), 0);
    }

    #[test]
    fn needle_longer_than_line_no_match() {
        let term = mock_term("ab");
        assert!(search_term(&term, "abc", SearchOptions::default()).is_empty());
    }

    #[test]
    fn grid_text_snapshot_matches_live_term_layout() {
        let term = mock_term("ab\ncd");
        let text = GridText::from_terminal(&term);
        assert_eq!(text.num_cols, usize::from(term.screen().cols()));
        assert_eq!(text.chars.len() % text.num_cols, 0);
        // The copy is keyed by RowId: the oldest stored row is the screen top
        // shifted back by the history depth.
        assert_eq!(text.oldest, term.screen().oldest());
        assert_eq!(text.screen_top, term.screen().screen_top());
        assert_eq!(
            text.screen_top.distance(text.oldest),
            u64::from(term.screen().history_len())
        );
        // Searching the snapshot after the term is gone still yields grid coordinates.
        drop(term);
        let m = search_grid_text(&text, "cd", SearchOptions::default());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].line, 1);
        assert_eq!(m[0].start_col, 0);
    }

    /// The history rows keep the negative lines the view converts with
    /// `display_row`, now derived from `RowId` rather than from a stored `i32`.
    #[test]
    fn history_rows_report_negative_grid_lines() {
        let mut term = crate::test_engine::terminal(GridSize { cols: 6, lines: 2 });
        crate::test_engine::feed(&mut term, b"alpha\r\nbeta\r\nalpha");
        let m = search_term(&term, "alpha", SearchOptions::default());
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].line, -1, "scrolled into history");
        assert_eq!(m[1].line, 1);
        assert_eq!(
            m[0].display_row(1),
            0,
            "one row of scrollback brings it back"
        );
    }

    #[test]
    fn search_skips_wide_char_spacer() {
        // mock_term handles wide chars by inserting WIDE_CHAR_SPACER after them.
        // "日本" (2 wide chars = 4 cells). Searching for "日" matches at col 0 only.
        let term = mock_term("日本");
        let m = search_term(&term, "日", SearchOptions::default());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[0].end_col, 1);
    }
}
