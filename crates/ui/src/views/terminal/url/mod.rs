//! URL detection in the terminal grid — always-on highlight + Ctrl+click to open URL.
//!
//! Two kinds of URL:
//! 1. **OSC 8 hyperlink** — the shell sends the escape sequence `\e]8;;URL\e\\` →
//!    alacritty_terminal attaches a `Hyperlink` to the cell. Available via `cell.hyperlink()`.
//! 2. **Plain text URL** — `http://...`, `https://...`, `www.` appearing in
//!    output text. Must scan cells to detect.

mod detect;
mod mask;

pub use detect::detect_url_at;
pub use mask::{url_column_mask, url_masks_wrapped};

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

/// URL protocol prefixes recognised in plain-text scanning.
pub(super) const PREFIXES: &[&[char]] = &[
    &['h', 't', 't', 'p', 's', ':', '/', '/'],
    &['h', 't', 't', 'p', ':', '/', '/'],
    &['f', 't', 'p', ':', '/', '/'],
    &['w', 'w', 'w', '.'],
];

pub(super) fn is_trailing_punct(c: char) -> bool {
    matches!(
        c,
        ')' | ']' | '}' | '>' | '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '`'
    )
}

#[cfg(test)]
mod tests;