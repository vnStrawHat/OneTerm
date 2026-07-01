//! URL detection in the terminal grid — Ctrl+hover highlight + Ctrl+click to open URL.
//!
//! Two kinds of URL:
//! 1. **OSC 8 hyperlink** — the shell sends the escape sequence `\e]8;;URL\e\\` →
//!    alacritty_terminal attaches a `Hyperlink` to the cell. Available via `cell.hyperlink()`.
//! 2. **Plain text URL** — `http://...`, `https://...`, `www.` appearing in
//!    output text. Must scan cells to detect.

use oneterm_core::terminal::IndexedCell;

/// A URL detected at a position in the terminal.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedUrl {
    /// URL string (may add an `https://` prefix when it is `www.`).
    pub url: String,
    /// Display row (0-based from top of viewport).
    pub row: usize,
    /// Start column (0-based, inclusive).
    pub start_col: usize,
    /// End column (0-based, exclusive).
    pub end_col: usize,
}

/// Find a URL at (row, col) in the terminal snapshot.
///
/// Check order:
/// 1. OSC 8 hyperlink — cell has `hyperlink()` → find all cells with the same
///    hyperlink ID on the same line.
/// 2. Plain text URL — scan the line for `http://`, `https://`, `ftp://`, `www.`.
pub fn detect_url_at(
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
    let line_cells = &cells[line_start..line_end];
    let n = line_cells.len();
    if col >= n {
        return None;
    }

    // 1. OSC 8 hyperlink — cell has hyperlink() → find the range with the same ID on the line.
    if let Some(h) = line_cells[col].cell.hyperlink() {
        let target_id = h.id();
        let mut start = col;
        let mut end = col;
        // Scan left.
        while start > 0 {
            if let Some(h2) = line_cells[start - 1].cell.hyperlink() {
                if h2.id() == target_id {
                    start -= 1;
                    continue;
                }
            }
            break;
        }
        // Scan right.
        while end < n - 1 {
            if let Some(h2) = line_cells[end + 1].cell.hyperlink() {
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
            end_col: end + 1, // exclusive
        });
    }

    // 2. Plain text URL detection.
    let chars: Vec<char> = line_cells
        .iter()
        .map(|ic| {
            if ic
                .cell
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER)
            {
                '\0'
            } else {
                ic.cell.c
            }
        })
        .collect();

    // Skip if the char at col is whitespace or null.
    if chars[col].is_whitespace() || chars[col] == '\0' {
        return None;
    }

    const PREFIXES: &[&[char]] = &[
        &['h', 't', 't', 'p', 's', ':', '/', '/'],
        &['h', 't', 't', 'p', ':', '/', '/'],
        &['f', 't', 'p', ':', '/', '/'],
        &['w', 'w', 'w', '.'],
    ];

    // Search backwards from col to find a URL prefix.
    for start in (0..=col).rev() {
        for prefix in PREFIXES {
            let plen = prefix.len();
            if start + plen > n {
                continue;
            }
            // Check prefix match.
            let matches = prefix
                .iter()
                .zip(&chars[start..start + plen])
                .all(|(a, b)| a == b);
            if !matches {
                continue;
            }

            // Found prefix at `start`. Extend the URL to whitespace or end of line.
            let mut end = start + plen;
            while end < n && !chars[end].is_whitespace() && chars[end] != '\0' {
                end += 1;
            }

            // col must lie within [start, end).
            if col < start || col >= end {
                continue;
            }

            // Strip trailing punctuation.
            while end > start + plen && is_trailing_punct(chars[end - 1]) {
                end -= 1;
            }

            if end <= start + plen {
                continue; // URL has only a prefix, no content.
            }

            let url: String = chars[start..end].iter().collect();
            let final_url = if url.starts_with("www.") {
                format!("https://{url}")
            } else {
                url
            };

            return Some(DetectedUrl {
                url: final_url,
                row,
                start_col: start,
                end_col: end,
            });
        }
    }

    None
}

fn is_trailing_punct(c: char) -> bool {
    matches!(
        c,
        ')' | ']' | '}' | '>' | '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '`'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cells(text: &str, num_cols: usize) -> Vec<IndexedCell> {
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::term::cell::Cell;

        let mut cells = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        for row in 0..(chars.len().div_ceil(num_cols)) {
            for col in 0..num_cols {
                let idx = row * num_cols + col;
                let c = if idx < chars.len() { chars[idx] } else { ' ' };
                let mut cell = Cell::default();
                cell.c = c;
                cells.push(IndexedCell {
                    point: alacritty_terminal::index::Point::new(Line(row as i32), Column(col)),
                    cell,
                });
            }
        }
        cells
    }

    #[test]
    fn detect_https_url() {
        let cells = make_cells("visit https://example.com today", 30);
        // "https://example.com" starts at col 6, ends at col 25
        let url = detect_url_at(&cells, 30, 0, 10).unwrap();
        assert_eq!(url.url, "https://example.com");
        assert_eq!(url.start_col, 6);
        assert_eq!(url.end_col, 25);
    }

    #[test]
    fn detect_http_url_at_start() {
        let cells = make_cells("http://foo.bar/baz", 20);
        let url = detect_url_at(&cells, 20, 0, 0).unwrap();
        assert_eq!(url.url, "http://foo.bar/baz");
    }

    #[test]
    fn detect_www_url_adds_https() {
        let cells = make_cells("see www.google.com here", 25);
        let url = detect_url_at(&cells, 25, 0, 5).unwrap();
        assert_eq!(url.url, "https://www.google.com");
    }

    #[test]
    fn strip_trailing_punctuation() {
        let cells = make_cells("link: https://example.com.", 26);
        let url = detect_url_at(&cells, 26, 0, 10).unwrap();
        assert_eq!(url.url, "https://example.com");
    }

    #[test]
    fn no_url_in_plain_text() {
        let cells = make_cells("hello world foo bar", 20);
        assert!(detect_url_at(&cells, 20, 0, 5).is_none());
    }

    #[test]
    fn no_url_on_whitespace() {
        // Click on space after URL (if any)
        let cells2 = make_cells("text https://x.com ", 20);
        assert!(detect_url_at(&cells2, 20, 0, 19).is_none()); // col 19 = space
    }
}
