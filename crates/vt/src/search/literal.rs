//! The character scanner: a literal needle, cell by cell, one row at a time.

use super::{GridText, SearchMatch, SearchOptions};
use crate::grid::RowId;

/// Find every occurrence of `query` in `text`, top to bottom.
pub(super) fn search(text: &GridText, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() || needle.len() > text.num_cols {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for (index, line_chars) in text.chars.chunks_exact(text.num_cols).enumerate() {
        find_in_line(
            line_chars,
            &needle,
            text.row_id(index),
            options,
            &mut matches,
        );
    }
    matches
}

/// Find all (non-overlapping) occurrences of `needle` in a single row's char
/// buffer, appending matches for `row` to `out`.
fn find_in_line(
    line: &[char],
    needle: &[char],
    row: RowId,
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
                    row,
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
