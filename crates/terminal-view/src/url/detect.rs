//! Click-time URL detection — finds the URL under the pointer for Ctrl+Click.
//!
//! Works on the `query_line_range_cells` slice around the pointer (a few
//! rows, never the whole grid), reading cells through the view-owned
//! [`Cell`] so the engine's cell type stays inside `render/frame.rs`.

use oneterm_terminal::IndexedCell;

use super::{DetectedUrl, PREFIXES, is_trailing_punct};
use crate::render::frame::{Cell, CellFlags, hyperlink_uri};

/// Find a URL at display `(row, col)` in `cells` (row-major, `num_cols` wide).
///
/// Handles URLs that wrap across display rows: when a row ends with
/// `WRAPLINE` set and the URL extends to its last column, detection continues
/// on the next row.
///
/// Check order:
/// 1. OSC 8 hyperlink — the cell carries a target → the run of cells with the
///    same link on the same row is the URL.
/// 2. Plain text URL — scan the wrapped row group for `http://`, `https://`,
///    `ftp://`, `www.`.
pub(crate) fn detect_url_at(
    cells: &[IndexedCell],
    num_cols: usize,
    row: usize,
    col: usize,
) -> Option<DetectedUrl> {
    let line_start = row * num_cols;
    let line_end = (line_start + num_cols).min(cells.len());
    if line_start >= cells.len() {
        return None;
    }
    let n = line_end - line_start;
    if col >= n {
        return None;
    }
    let at = |i: usize| Cell::from_indexed(&cells[i]);
    let is_spacer = |c: &Cell<'_>| c.flags.contains(CellFlags::WIDE_CHAR_SPACER);
    let visible_char = |c: &Cell<'_>| match c.ch {
        '\0' | '\t' => ' ',
        ch => ch,
    };

    // 1. OSC 8 hyperlink — per-row.
    if let Some(target) = at(line_start + col).hyperlink {
        let same_link = |i: usize| at(i).hyperlink == Some(target);
        let mut start = col;
        while start > 0 && same_link(line_start + start - 1) {
            start -= 1;
        }
        let mut end = col;
        while end < n - 1 && same_link(line_start + end + 1) {
            end += 1;
        }
        // The visible label of the link, so the click handler can compare it
        // with the target (SEC-03).
        let display_text: String = (line_start + start..=line_start + end)
            .map(at)
            .filter(|c| !is_spacer(c))
            .map(|c| c.ch)
            .collect();
        return Some(DetectedUrl {
            url: hyperlink_uri(&cells[line_start + col])?,
            display_text: Some(display_text),
            row,
            start_col: start,
            end_col: end + 1,
        });
    }

    // 2. Plain-text URL — wrap-aware. Find the start of the wrapped row group
    // (scan backwards for WRAPLINE on the previous row's last cell).
    let mut group_start = row;
    while group_start > 0 {
        let prev_end = group_start * num_cols;
        if prev_end > 0 && at(prev_end - 1).flags.contains(CellFlags::WRAPLINE) {
            group_start -= 1;
        } else {
            break;
        }
    }

    // Build chars + position map for the entire wrapped group.
    let mut chars: Vec<char> = Vec::new();
    let mut pos_map: Vec<(usize, usize)> = Vec::new();

    let mut current_row = group_start;
    loop {
        let ls = current_row * num_cols;
        let le = (ls + num_cols).min(cells.len());
        if ls >= cells.len() {
            break;
        }
        let mut wraps = false;
        for c in 0..num_cols {
            let idx = ls + c;
            if idx >= le {
                break;
            }
            let cell = at(idx);
            if is_spacer(&cell) {
                continue;
            }
            pos_map.push((current_row, c));
            chars.push(visible_char(&cell));
            if c == num_cols - 1 && cell.flags.contains(CellFlags::WRAPLINE) {
                wraps = true;
            }
        }
        if !wraps {
            break;
        }
        current_row += 1;
    }

    let click_idx = pos_map.iter().position(|(r, c)| *r == row && *c == col)?;
    if chars[click_idx].is_whitespace() || chars[click_idx] == '\0' {
        return None;
    }

    // Search backwards from the click for a URL prefix.
    for start in (0..=click_idx).rev() {
        for prefix in PREFIXES {
            let plen = prefix.len();
            if start + plen > chars.len() {
                continue;
            }
            let matches = prefix
                .iter()
                .zip(&chars[start..start + plen])
                .all(|(a, b)| *a == *b);
            if !matches {
                continue;
            }

            // Found a prefix: extend to whitespace or the end of the group.
            let mut end = start + plen;
            while end < chars.len() && !chars[end].is_whitespace() && chars[end] != '\0' {
                end += 1;
            }
            if click_idx < start || click_idx >= end {
                continue;
            }
            while end > start + plen && is_trailing_punct(chars[end - 1]) {
                end -= 1;
            }
            if end <= start + plen {
                continue;
            }

            let url: String = chars[start..end].iter().collect();
            let final_url = if url.starts_with("www.") {
                format!("https://{url}")
            } else {
                url
            };
            let (url_row, url_start_col) = pos_map[start];
            let (_, url_end_col) = pos_map[end - 1];
            return Some(DetectedUrl {
                url: final_url,
                display_text: None,
                row: url_row,
                start_col: url_start_col,
                end_col: url_end_col + 1,
            });
        }
    }

    None
}
