//! URL detection in the terminal grid — always-on highlight + Ctrl+click to open URL.
//!
//! Two kinds of URL:
//! 1. **OSC 8 hyperlink** — the shell sends the escape sequence `\e]8;;URL\e\\` →
//!    alacritty_terminal attaches a `Hyperlink` to the cell. Available via `cell.hyperlink()`.
//! 2. **Plain text URL** — `http://...`, `https://...`, `www.` appearing in
//!    output text. Must scan cells to detect.

mod detect;
mod hover;
mod mask;

pub(crate) use detect::detect_url_at;
pub(crate) use hover::UrlHover;
pub(crate) use mask::url_masks_wrapped;

/// A URL detected at a position in the terminal.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetectedUrl {
    /// URL string (may add an `https://` prefix when it is `www.`).
    pub url: String,
    /// The visible text of an OSC 8 hyperlink (its cells), which may differ
    /// from `url`; `None` for plain-text URLs, whose text *is* the URL. The
    /// click handler asks the target policy to confirm mismatches (SEC-03).
    pub display_text: Option<String>,
    /// Display row (0-based from top of viewport).
    pub row: usize,
    /// Start column (0-based, inclusive).
    pub start_col: usize,
    /// End column (0-based, exclusive).
    pub end_col: usize,
}

/// Number of display rows queried on either side of the pointer for URL
/// detection: enough for a wrapped URL, far cheaper than cloning the grid.
pub(crate) const URL_WINDOW: usize = 5;

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
