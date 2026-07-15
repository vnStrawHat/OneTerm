//! Detect URLs / hyperlinks within a row of terminal cells.
//!
//! Two sources: (1) OSC 8 hyperlinks attached to cells by the app; (2) plain-text
//! URLs in the displayed text (via `linkify`). See `freya-terminal/url.rs`.

use alacritty_terminal::term::cell::{Cell, Flags};
use linkify::{LinkFinder, LinkKind};

thread_local! {
    static FINDER: LinkFinder = {
        let mut f = LinkFinder::new();
        f.kinds(&[LinkKind::Url]);
        f
    };
}

/// Column ranges `[start_col, end_col)` of the clickable runs in `row`:
/// OSC 8 hyperlinks + plain-text URLs (linkify).
pub fn link_ranges(row: &[Cell]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    // OSC 8 hyperlink: consecutive cells sharing the same hyperlink.
    let mut run_start: Option<usize> = None;
    for (col, cell) in row.iter().enumerate() {
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        if cell.hyperlink().is_some() {
            run_start.get_or_insert(col);
        } else if let Some(start) = run_start.take() {
            ranges.push((start, col));
        }
    }
    if let Some(start) = run_start {
        ranges.push((start, row.len()));
    }

    // Plain-text URL.
    if row_has_url_marker(row) {
        let (text, byte_to_col) = row_text(row);
        FINDER.with(|f| {
            for link in f.links(&text) {
                ranges.push((byte_to_col[link.start()], byte_to_col[link.end() - 1] + 1));
            }
        });
    }

    ranges
}

/// The URL at column `col` in `row`, if any. Prefers OSC 8 hyperlinks, then
/// plain-text URLs.
pub fn url_at(row: &[Cell], col: usize) -> Option<String> {
    // OSC 8 hyperlink directly.
    if col < row.len() {
        if let Some(h) = row[col].hyperlink() {
            return Some(h.uri().to_owned());
        }
    }
    // Plain-text URL.
    if !row_has_url_marker(row) {
        return None;
    }
    let (text, byte_to_col) = row_text(row);
    FINDER.with(|f| {
        f.links(&text).find_map(|link| {
            let start = byte_to_col[link.start()];
            let end = byte_to_col[link.end() - 1] + 1;
            (col >= start && col < end).then(|| link.as_str().to_owned())
        })
    })
}

/// Cheap pre-scan: skip allocating text when the row has no `://` triplet.
fn row_has_url_marker(row: &[Cell]) -> bool {
    let (mut a, mut b) = ('\0', '\0');
    for cell in row
        .iter()
        .filter(|c| !c.flags.contains(Flags::WIDE_CHAR_SPACER))
    {
        if a == ':' && b == '/' && cell.c == '/' {
            return true;
        }
        a = b;
        b = cell.c;
    }
    false
}

/// The displayed text of the row plus a byte→column map. Skips wide-char spacers
/// to match the renderer's layout.
fn row_text(row: &[Cell]) -> (String, Vec<usize>) {
    let mut text = String::with_capacity(row.len());
    let mut byte_to_col = Vec::with_capacity(row.len());
    for (col, cell) in row.iter().enumerate() {
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let c = match cell.c {
            '\0' | '\t' => ' ',
            c => c,
        };
        text.push(c);
        byte_to_col.resize(text.len(), col);
    }
    (text, byte_to_col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::cell::Cell;

    fn row_from_str(s: &str) -> Vec<Cell> {
        s.chars()
            .map(|c| {
                let mut cell = Cell::default();
                cell.c = c;
                cell
            })
            .collect()
    }

    #[test]
    fn no_url_no_ranges() {
        let row = row_from_str("hello world");
        assert!(link_ranges(&row).is_empty());
        assert!(url_at(&row, 3).is_none());
    }

    #[test]
    fn detects_https_url() {
        let row = row_from_str("see https://example.com now");
        let ranges = link_ranges(&row);
        assert_eq!(ranges.len(), 1);
        // "see " = 4 characters → the URL starts at column 4.
        assert_eq!(ranges[0].0, 4);
        let url = url_at(&row, 10).unwrap();
        assert_eq!(url, "https://example.com");
    }

    #[test]
    fn no_marker_skips_linkify() {
        // A bare domain without `://` → marker false → skip linkify → None.
        let row = row_from_str("see www.example.com here");
        assert!(url_at(&row, 5).is_none());
    }

    #[test]
    fn ftp_scheme_is_url() {
        // linkify's Url kind matches any `://` scheme (including ftp) → still a URL.
        let row = row_from_str("ftp://host/x");
        assert!(url_at(&row, 4).is_some());
    }
}
