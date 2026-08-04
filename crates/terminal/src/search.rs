//! Terminal scrollback search — framework-agnostic algorithm operating on `Term`.
//!
//! The UI asks a `TerminalSession` for matches (`fn search`); the backend locks
//! its `Term` and calls [`search_term`] here. Matches are reported in **grid
//! coordinates** (alacritty `Line.0`): negative values are scrollback history,
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

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};

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
/// `line` is the alacritty `Line.0` value:
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

/// Search the full grid (scrollback history + viewport) of `term` for `query`.
///
/// Returns matches in **top-to-bottom order** (oldest history first, newest last)
/// so forward navigation ("next") walks down the scrollback.
///
/// Empty `query` → empty result. The query is matched against the per-line text
/// (cells joined left-to-right); matches do **not** span line boundaries.
pub(crate) fn search_term<EP: EventListener>(
    term: &Term<EP>,
    query: &str,
    options: SearchOptions,
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let grid = term.grid();
    let num_cols = grid.columns();
    if num_cols == 0 {
        return Vec::new();
    }

    let needle: Vec<char> = query.chars().collect();
    let needle_len = needle.len();
    if needle_len == 0 || needle_len > num_cols {
        return Vec::new();
    }

    let top = grid.topmost_line().0;
    let bottom = grid.bottommost_line().0;
    let mut matches = Vec::new();

    // Reusable line buffer (one char per column).
    let mut line_chars: Vec<char> = Vec::with_capacity(num_cols);

    for line in top..=bottom {
        let row = &grid[Line(line)];
        line_chars.clear();
        for col in 0..num_cols {
            let cell: &Cell = &row[Column(col)];
            // Wide-char spacers carry no visible glyph — use a NUL placeholder so
            // they cannot be part of a match (the needle never contains NUL). This
            // keeps the column index aligned with the cell column.
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER) {
                line_chars.push('\0');
            } else {
                line_chars.push(cell.c);
            }
        }

        find_in_line(&line_chars, &needle, line, options, &mut matches);
    }

    matches
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
    use alacritty_terminal::term::test::mock_term;

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
        let cols = term.grid().columns();
        // "foo bar foo baz foo" — three "foo".
        assert_eq!(cols, 19);
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
