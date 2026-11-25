//! The regular-expression matcher, one row at a time.
//!
//! Each row's `char`s are joined into a `String` and matched there, then the
//! byte offsets the `regex` crate reports are mapped back to cell columns. One
//! `String` and one offset table are reused across rows.

use super::{GridText, SearchMatch, SearchOptions};

/// Find every match of `re` in `text`, top to bottom.
pub(super) fn search(
    text: &GridText,
    re: &::regex::Regex,
    options: SearchOptions,
) -> Vec<SearchMatch> {
    debug_assert!(
        options == SearchOptions::default(),
        "SearchOptions are ignored for a regex pattern: put (?i) or \\b in the pattern itself"
    );

    let mut matches = Vec::new();
    let mut line = String::with_capacity(text.num_cols * 4);
    // `starts[col]` is the byte offset of column `col` in `line`, with one
    // sentinel past the end so an exclusive match end maps too.
    let mut starts: Vec<usize> = Vec::with_capacity(text.num_cols + 1);

    for (index, chars) in text.chars.chunks_exact(text.num_cols).enumerate() {
        line.clear();
        starts.clear();
        for &c in chars {
            starts.push(line.len());
            line.push(c);
        }
        starts.push(line.len());

        let row = text.row_id(index);
        // `find_iter` yields non-overlapping matches and advances by one
        // character past an empty match, so a pattern matching the empty
        // string terminates.
        for m in re.find_iter(&line) {
            matches.push(SearchMatch {
                row,
                start_col: column_of(&starts, m.start()),
                end_col: column_of(&starts, m.end()),
            });
        }
    }
    matches
}

/// The column whose character starts at byte offset `at`.
///
/// Every match boundary is a character boundary, so `at` is one of the recorded
/// offsets and the search lands on it exactly.
fn column_of(starts: &[usize], at: usize) -> usize {
    starts.partition_point(|&start| start < at)
}
