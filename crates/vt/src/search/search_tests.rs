//! `search::tests::` — the literal scanner, and the regex matcher behind the
//! `regex` feature.

use std::time::Instant;

use super::*;
use crate::event::EventBatch;
use crate::grid::Size;
use crate::terminal::Config;

/// A fresh terminal at `cols` x `rows` with the default scrollback, fed
/// `bytes`.
fn terminal(cols: usize, rows: usize, bytes: &[u8]) -> Terminal {
    let mut term = Terminal::new(
        Size {
            rows: rows as u16,
            cols: cols as u16,
        },
        Config::default(),
    );
    term.feed(bytes, &mut EventBatch::new(), Instant::now());
    term
}

/// The `mock_term` contract the twelve tests below were written against:
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
                .map(|c| usize::from(crate::width::scalar_width(c).unwrap_or(0)))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(1)
        .max(1);
    terminal(cols, lines.len(), lines.join("\r\n").as_bytes())
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
    assert_eq!(m[0].row, term.screen().screen_top());
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
    let top = term.screen().screen_top();
    let m = search_term(&term, "alpha", SearchOptions::default());
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].grid_line(top), 0);
    assert_eq!(m[1].grid_line(top), 2);
    assert_eq!(m[1].row, top + 2);
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
    let top = term.screen().screen_top();
    let m = search_term(&term, "hello", SearchOptions::default());
    assert_eq!(m.len(), 1);
    // No scrollback (display_offset = 0) → display row = grid line.
    assert_eq!(m[0].display_row(top, 0), 0);
}

#[test]
fn needle_longer_than_line_no_match() {
    let term = mock_term("ab");
    assert!(search_term(&term, "abc", SearchOptions::default()).is_empty());
}

#[test]
fn grid_text_snapshot_matches_live_term_layout() {
    let term = mock_term("ab\ncd");
    let top = term.screen().screen_top();
    let text = GridText::from_terminal(&term);
    assert_eq!(text.num_cols, usize::from(term.screen().cols()));
    assert_eq!(text.chars.len() % text.num_cols, 0);
    assert_eq!(text.rows(), 2);
    // The copy is keyed by RowId: the oldest stored row is the screen top
    // shifted back by the history depth.
    assert_eq!(text.oldest(), term.screen().oldest());
    assert_eq!(
        top.distance(text.oldest()),
        u64::from(term.screen().history_len())
    );
    // Searching the snapshot after the term is gone still names live rows.
    drop(term);
    let m = search_grid_text(
        &text,
        SearchPattern::Literal("cd"),
        SearchOptions::default(),
    );
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].row, top + 1);
    assert_eq!(m[0].grid_line(top), 1);
    assert_eq!(m[0].start_col, 0);
}

/// A match in history is below `screen_top`, which is what makes its grid
/// line negative and what the view's `display_row` adds the offset to.
#[test]
fn history_rows_report_negative_grid_lines() {
    let term = terminal(6, 2, b"alpha\r\nbeta\r\nalpha");
    let top = term.screen().screen_top();
    let m = search_term(&term, "alpha", SearchOptions::default());
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].grid_line(top), -1, "scrolled into history");
    assert_eq!(m[1].grid_line(top), 1);
    assert_eq!(
        m[0].display_row(top, 1),
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

#[cfg(feature = "regex")]
mod regex_pattern {
    use super::*;

    fn search_re(term: &Terminal, pattern: &str) -> Vec<SearchMatch> {
        let re = ::regex::Regex::new(pattern).expect("test pattern compiles");
        search_grid_text(
            &GridText::from_terminal(term),
            SearchPattern::Regex(&re),
            SearchOptions::default(),
        )
    }

    /// A pattern with no metacharacters is the literal scanner's answer, to
    /// the column: the feature adds a matcher, it does not change the old one.
    #[test]
    fn literal_equivalent_pattern_matches_the_literal_scanner() {
        let term = mock_term("foo bar foo baz foo");
        let literal = search_term(&term, "foo", SearchOptions::default());
        assert_eq!(search_re(&term, "foo"), literal);
    }

    #[test]
    fn anchor_matches_only_at_column_zero() {
        let term = mock_term("foo foo");
        let m = search_re(&term, "^foo");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[0].end_col, 3);
    }

    /// An empty-matching pattern must terminate, must not report two matches at
    /// one column, and -- the documented sharp edge -- reports its last match at
    /// the position past the last cell, so `start_col` equals the column count.
    #[test]
    fn empty_matching_pattern_terminates_and_advances() {
        let term = mock_term("ab");
        let m = search_re(&term, "x*");
        assert_eq!(m.len(), 3, "one empty match per position, plus the end");
        let cols: Vec<usize> = m.iter().map(|hit| hit.start_col).collect();
        assert_eq!(cols, vec![0, 1, 2]);
        assert!(m.iter().all(|hit| hit.end_col == hit.start_col));
        assert_eq!(
            cols[2],
            usize::from(term.screen().cols()),
            "the last empty match sits one past the last column"
        );
    }

    /// The row is the whole grid row, so a match can land on the blank cells
    /// past the last glyph and `$` anchors to the last column.
    #[test]
    fn a_row_is_padded_to_the_grid_width() {
        let term = terminal(5, 1, b"ab");
        let m = search_re(&term, r"b\s+$");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_col, 1);
        assert_eq!(m[0].end_col, 5);
    }

    /// `case_sensitive` and `whole_word` are the literal scanner's; a regex
    /// owns its own flags, so passing a non-default option is a bug the
    /// debug assertion names.
    #[test]
    #[should_panic(expected = "SearchOptions are ignored for a regex pattern")]
    #[cfg(debug_assertions)]
    fn non_default_options_with_a_regex_trip_the_assertion() {
        let term = mock_term("foo");
        let re = ::regex::Regex::new("foo").expect("test pattern compiles");
        let opts = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };
        let _ = search_grid_text(
            &GridText::from_terminal(&term),
            SearchPattern::Regex(&re),
            opts,
        );
    }
}
