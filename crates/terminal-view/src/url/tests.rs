//! Tests for URL detection and mask computation. Grids are built with the
//! frame test builder, so no engine type is named here.

use oneterm_terminal::IndexedCell;

use super::*;
use crate::render::frame::test_support::FrameBuilder;
use crate::render::frame::{CellFlags, Frame};

/// `text` laid out row-major over `num_cols` columns (at least one row),
/// with `WRAPLINE` on the last cell of every row listed in `wraps`.
fn grid(text: &str, num_cols: usize, wraps: &[usize]) -> FrameBuilder {
    let chars: Vec<char> = text.chars().collect();
    let rows = chars.len().div_ceil(num_cols).max(1);
    let mut builder = FrameBuilder::new(rows, num_cols);
    for (row, chunk) in chars.chunks(num_cols).enumerate() {
        let line: String = chunk.iter().collect();
        builder = builder.text(row, 0, &line);
    }
    for &row in wraps {
        builder = builder.flags(row, num_cols - 1, CellFlags::WRAPLINE);
    }
    builder
}

fn make_cells(text: &str, num_cols: usize) -> Vec<IndexedCell> {
    grid(text, num_cols, &[]).into_cells()
}

fn wrapped_cells(text: &str, num_cols: usize, wraps: &[usize]) -> Vec<IndexedCell> {
    grid(text, num_cols, wraps).into_cells()
}

fn wrapped_frame(text: &str, num_cols: usize, wraps: &[usize]) -> Frame {
    grid(text, num_cols, wraps).build()
}

fn make_osc8_cells(display: &str, target: &str) -> Vec<IndexedCell> {
    let n = display.chars().count().max(1);
    grid(display, n, &[])
        .hyperlink_run(0, 0, n, target)
        .into_cells()
}

fn masks(frame: &Frame) -> Vec<Vec<bool>> {
    let mut out = Vec::new();
    let mut wraps = Vec::new();
    url_masks_into(frame, &mut out, &mut wraps);
    out
}

// --- detect_url_at tests ---

#[test]
fn detect_https_url() {
    let cells = make_cells("visit https://example.com today", 30);
    let url = detect_url_at(&cells, 30, 0, 10).expect("url");
    assert_eq!(url.url, "https://example.com");
    assert_eq!(url.start_col, 6);
    assert_eq!(url.end_col, 25);
}

#[test]
fn detect_http_url_at_start() {
    let cells = make_cells("http://foo.bar/baz", 20);
    let url = detect_url_at(&cells, 20, 0, 0).expect("url");
    assert_eq!(url.url, "http://foo.bar/baz");
}

#[test]
fn detect_www_url_adds_https() {
    let cells = make_cells("see www.google.com here", 25);
    let url = detect_url_at(&cells, 25, 0, 5).expect("url");
    assert_eq!(url.url, "https://www.google.com");
}

#[test]
fn strip_trailing_punctuation() {
    let cells = make_cells("link: https://example.com.", 26);
    let url = detect_url_at(&cells, 26, 0, 10).expect("url");
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
fn phase1_osc8_targets_are_policy_validated() {
    use oneterm_terminal::url_policy::{TargetDecision, validate_target};

    // Denied: custom application scheme.
    let cells = make_osc8_cells("click me", "custom-app://run?action=delete");
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert_eq!(detected.display_text.as_deref(), Some("click me"));
    assert_eq!((detected.start_col, detected.end_col), (0, cells.len()));
    assert!(matches!(
        validate_target(&detected.url),
        TargetDecision::Deny(_)
    ));

    // Allowed: mixed-case HTTPS (case-insensitive scheme).
    let cells = make_osc8_cells("HTTPS link", "HtTpS://Example.COM/Path");
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert_eq!(validate_target(&detected.url), TargetDecision::Allow);

    // Allowed: Unicode host.
    let cells = make_osc8_cells("Unicode host", "https://例え.テスト/path");
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert_eq!(validate_target(&detected.url), TargetDecision::Allow);

    // Denied: credentials in authority.
    let cells = make_osc8_cells(
        "credential target",
        "https://user:secret@example.com/private",
    );
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert!(matches!(
        validate_target(&detected.url),
        TargetDecision::Deny(_)
    ));

    // Denied: file scheme.
    let cells = make_osc8_cells("safe label", "file:///C:/Windows/System32/cmd.exe");
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert!(matches!(
        validate_target(&detected.url),
        TargetDecision::Deny(_)
    ));

    // Denied: control character in URL.
    let cells = make_osc8_cells("safe label", "https://example.com/\u{0007}control");
    let detected = detect_url_at(&cells, cells.len(), 0, 0).expect("link");
    assert!(matches!(
        validate_target(&detected.url),
        TargetDecision::Deny(_)
    ));

    // Denied: oversized URL.
    let oversized = format!("https://example.com/{}", "x".repeat(256 * 1024));
    let cells = make_osc8_cells("short display text", &oversized);
    let detected = detect_url_at(&cells, cells.len(), 0, 3).expect("link");
    assert!(matches!(
        validate_target(&detected.url),
        TargetDecision::Deny(_)
    ));
}

// --- Wrap-aware tests ---

#[test]
fn detect_wrapped_url_click_first_line() {
    let cells = wrapped_cells("https://x.com/path", 10, &[0]);
    let url = detect_url_at(&cells, 10, 0, 2).expect("url");
    assert_eq!(url.url, "https://x.com/path");
}

#[test]
fn detect_wrapped_url_click_second_line() {
    let cells = wrapped_cells("https://x.com/path", 10, &[0]);
    let url = detect_url_at(&cells, 10, 1, 3).expect("url");
    assert_eq!(url.url, "https://x.com/path");
}

#[test]
fn detect_wrapped_url_three_lines() {
    let cells = wrapped_cells("https://x.com/very/long/path", 10, &[0, 1]);
    let url = detect_url_at(&cells, 10, 2, 0).expect("url");
    assert_eq!(url.url, "https://x.com/very/long/path");
}

#[test]
fn masks_wrapped_url_extends_to_next_line() {
    let masks = masks(&wrapped_frame("https://x.com/path", 10, &[0]));
    assert!(masks[0].iter().all(|&v| v), "line 0 should be all URL");
    for col in 0..8 {
        assert!(masks[1][col], "line 1 col {col} should be URL");
    }
    assert!(!masks[1][8], "line 1 col 8 should not be URL");
    assert!(!masks[1][9], "line 1 col 9 should not be URL");
}

#[test]
fn masks_no_extend_when_url_does_not_reach_end() {
    // Only the first row is looked at: the URL ends before the row does.
    let masks = masks(&wrapped_frame("visit https://x.com ", 20, &[]));
    assert!(masks[0][6]);
    assert!(masks[0][18]);
    assert!(!masks[0][19], "col 19 is space, not URL");
}

#[test]
fn masks_strip_trailing_punct_on_non_wrapped_line() {
    let masks = masks(&wrapped_frame("link: https://example.com.", 26, &[]));
    assert!(masks[0][6]);
    assert!(masks[0][24]);
    assert!(!masks[0][25], "trailing dot should be stripped");
}
