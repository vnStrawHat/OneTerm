//! Click-time URL detection — finds the URL under the cursor for Ctrl+Click.

use oneterm_core::terminal::IndexedCell;

use super::{DetectedUrl, PREFIXES, is_trailing_punct};

/// Find a URL at (row, col) in the terminal snapshot.
///
/// Handles URLs that wrap across display lines: when a line ends with
/// `WRAPLINE` set and the URL extends to its last column, detection continues
/// on the next display line.
///
/// Check order:
/// 1. OSC 8 hyperlink — cell has `hyperlink()` → find all cells with the same
///    hyperlink ID on the same line.
/// 2. Plain text URL — scan the wrapped line group for `http://`, `https://`,
///    `ftp://`, `www.`.
pub fn detect_url_at(
    cells: &[IndexedCell],
    num_cols: usize,
    row: usize,
    col: usize,
) -> Option<DetectedUrl> {
    use alacritty_terminal::term::cell::Flags;

    let line_start = row * num_cols;
    let line_end = (line_start + num_cols).min(cells.len());
    if line_start >= cells.len() {
        return None;
    }
    let n = line_end - line_start;
    if col >= n {
        return None;
    }

    // 1. OSC 8 hyperlink — per-line, unchanged.
    if let Some(h) = cells[line_start + col].cell.hyperlink() {
        let target_id = h.id();
        let mut start = col;
        let mut end = col;
        while start > 0 {
            if let Some(h2) = cells[line_start + start - 1].cell.hyperlink() {
                if h2.id() == target_id {
                    start -= 1;
                    continue;
                }
            }
            break;
        }
        while end < n - 1 {
            if let Some(h2) = cells[line_start + end + 1].cell.hyperlink() {
                if h2.id() == target_id {
                    end += 1;
                    continue;
                }
            }
            break;
        }
        return Some(DetectedUrl {
            url: h.uri().to_string(),
            row,
            start_col: start,
            end_col: end + 1,
        });
    }

    // 2. Plain-text URL — wrap-aware.
    // Find the start of the wrapped line group (scan backwards for WRAPLINE).
    let mut group_start = row;
    while group_start > 0 {
        let prev_end = group_start * num_cols;
        if prev_end > 0 && cells[prev_end - 1].cell.flags.contains(Flags::WRAPLINE) {
            group_start -= 1;
        } else {
            break;
        }
    }

    // Build chars + position map for the entire wrapped group.
    let mut chars: Vec<char> = Vec::new();
    let mut pos_map: Vec<(usize, usize)> = Vec::new(); // (row, col) per char

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
            let cell = &cells[idx];
            if cell.cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let ch = match cell.cell.c {
                '\0' | '\t' => ' ',
                ch => ch,
            };
            pos_map.push((current_row, c));
            chars.push(ch);
            if c == num_cols - 1 && cell.cell.flags.contains(Flags::WRAPLINE) {
                wraps = true;
            }
        }
        if !wraps {
            break;
        }
        current_row += 1;
    }

    // Find the char index of the clicked (row, col).
    let click_idx = pos_map.iter().position(|(r, c)| *r == row && *c == col)?;

    // Skip if whitespace at click position.
    if chars[click_idx].is_whitespace() || chars[click_idx] == '\0' {
        return None;
    }

    // Search backwards from click_idx to find a URL prefix.
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

            // Found prefix. Extend to whitespace or end.
            let mut end = start + plen;
            while end < chars.len() && !chars[end].is_whitespace() && chars[end] != '\0' {
                end += 1;
            }

            if click_idx < start || click_idx >= end {
                continue;
            }

            // Strip trailing punctuation.
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
                row: url_row,
                start_col: url_start_col,
                end_col: url_end_col + 1,
            });
        }
    }

    None
}
