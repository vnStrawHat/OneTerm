//! Tests for URL detection and mask computation.

use super::*;
use oneterm_core::terminal::IndexedCell;

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

fn make_osc8_cells(display: &str, target: &str) -> Vec<IndexedCell> {
    use alacritty_terminal::term::cell::Hyperlink;

    let mut cells = make_cells(display, display.chars().count().max(1));
    let hyperlink = Hyperlink::new(Some("phase0-vector"), target.to_string());
    for cell in &mut cells {
        cell.cell.set_hyperlink(Some(hyperlink.clone()));
    }
    cells
}

// --- detect_url_at tests ---

#[test]
fn detect_https_url() {
    let cells = make_cells("visit https://example.com today", 30);
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
    let cells2 = make_cells("text https://x.com ", 20);
    assert!(detect_url_at(&cells2, 20, 0, 19).is_none());
}

#[test]
fn phase0_baseline_osc8_targets_are_returned_without_policy() {
    let vectors = [
        ("click me", "custom-app://run?action=delete"),
        ("HTTPS link", "HtTpS://Example.COM/Path"),
        ("Unicode host", "https://例え.テスト/path"),
        (
            "credential target",
            "https://user:secret@example.com/private",
        ),
        ("safe label", "file:///C:/Windows/System32/cmd.exe"),
        ("safe label", "https://example.com/\u{0007}control"),
    ];

    for (display, target) in vectors {
        let cells = make_osc8_cells(display, target);
        let detected = detect_url_at(&cells, cells.len(), 0, 0).unwrap();
        assert_eq!(detected.url, target);
    }

    let oversized = format!("https://example.com/{}", "x".repeat(256 * 1024));
    let cells = make_osc8_cells("short display text", &oversized);
    let detected = detect_url_at(&cells, cells.len(), 0, 3).unwrap();
    assert_eq!(detected.url, oversized);
}

// --- url_column_mask tests ---

fn make_line_refs(cells: &[IndexedCell], row: usize, num_cols: usize) -> Vec<&IndexedCell> {
    cells[row * num_cols..(row + 1) * num_cols].iter().collect()
}

#[test]
fn mask_detects_https_url() {
    let cells = make_cells("visit https://example.com today", 30);
    let line = make_line_refs(&cells, 0, 30);
    let mask = url_column_mask(&line);
    for col in 6..25 {
        assert!(mask[col], "col {col} should be URL");
    }
    for col in 0..6 {
        assert!(!mask[col], "col {col} should not be URL");
    }
}

#[test]
fn mask_detects_multiple_urls() {
    let cells = make_cells("http://a.com https://b.com", 25);
    let line = make_line_refs(&cells, 0, 25);
    let mask = url_column_mask(&line);
    for col in 0..12 {
        assert!(mask[col], "col {col} should be URL");
    }
    assert!(!mask[12]);
    for col in 13..25 {
        assert!(mask[col], "col {col} should be URL");
    }
}

#[test]
fn mask_strips_trailing_punct() {
    let cells = make_cells("link: https://example.com.", 26);
    let line = make_line_refs(&cells, 0, 26);
    let mask = url_column_mask(&line);
    assert!(mask[6]);
    assert!(!mask[25], "trailing dot should not be URL");
}

#[test]
fn mask_no_url_in_plain_text() {
    let cells = make_cells("hello world foo bar", 20);
    let line = make_line_refs(&cells, 0, 20);
    let mask = url_column_mask(&line);
    assert!(mask.iter().all(|&v| !v));
}

// --- Wrap-aware tests ---

/// Helper: set WRAPLINE on the last cell of `row`.
fn set_wrapline(cells: &mut [IndexedCell], row: usize, num_cols: usize) {
    use alacritty_terminal::term::cell::Flags;
    let idx = row * num_cols + num_cols - 1;
    cells[idx].cell.flags.insert(Flags::WRAPLINE);
}

#[test]
fn detect_wrapped_url_click_first_line() {
    let mut cells = make_cells("https://x.com/path", 10);
    set_wrapline(&mut cells, 0, 10);
    let url = detect_url_at(&cells, 10, 0, 2).unwrap();
    assert_eq!(url.url, "https://x.com/path");
}

#[test]
fn detect_wrapped_url_click_second_line() {
    let mut cells = make_cells("https://x.com/path", 10);
    set_wrapline(&mut cells, 0, 10);
    let url = detect_url_at(&cells, 10, 1, 3).unwrap();
    assert_eq!(url.url, "https://x.com/path");
}

#[test]
fn detect_wrapped_url_three_lines() {
    let text = "https://x.com/very/long/path";
    let mut cells = make_cells(text, 10);
    set_wrapline(&mut cells, 0, 10);
    set_wrapline(&mut cells, 1, 10);
    let url = detect_url_at(&cells, 10, 2, 0).unwrap();
    assert_eq!(url.url, "https://x.com/very/long/path");
}

#[test]
fn masks_wrapped_url_extends_to_next_line() {
    let mut cells = make_cells("https://x.com/path", 10);
    set_wrapline(&mut cells, 0, 10);
    let masks = url_masks_wrapped(&cells, 2, 10);
    assert!(masks[0].iter().all(|&v| v), "line 0 should be all URL");
    for col in 0..8 {
        assert!(masks[1][col], "line 1 col {col} should be URL");
    }
    assert!(!masks[1][8], "line 1 col 8 should not be URL");
    assert!(!masks[1][9], "line 1 col 9 should not be URL");
}

#[test]
fn masks_no_extend_when_url_does_not_reach_end() {
    let cells = make_cells("visit https://x.com now", 20);
    let masks = url_masks_wrapped(&cells, 1, 20);
    assert!(masks[0][6]);
    assert!(masks[0][18]);
    assert!(!masks[0][19], "col 19 is space, not URL");
}

#[test]
fn masks_strip_trailing_punct_on_non_wrapped_line() {
    let cells = make_cells("link: https://example.com.", 26);
    let masks = url_masks_wrapped(&cells, 1, 26);
    assert!(masks[0][6]);
    assert!(masks[0][24]);
    assert!(!masks[0][25], "trailing dot should be stripped");
}
