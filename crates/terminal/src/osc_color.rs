//! OSC dynamic-color support (OSC 10/11/12 set/query + OSC 110/111/112 reset).
//!
//! Alacritty's `Term` already parses these sequences: a *set* stores the color
//! in `Term.colors[index]`, a *reset* clears it (`None`), and a *query* is
//! routed out as `Event::ColorRequest(index, format)`. This module provides the
//! small shared types the backends need to:
//!
//! - **Render** OSC-set colors — read the live fg/bg/cursor overrides from the
//!   `Term` color table via [`DynamicColors`].
//! - **Answer queries** — collect pending [`PendingColorQuery`] items in the
//!   `EventListener`, then reply after the parse batch (when the `Term` lock is
//!   free to read the current color), falling back to the theme default via
//!   [`default_color_for_index`].
//!
//! Reference: alacritty `alacritty/src/event.rs` `TerminalEvent::ColorRequest`
//! handling + `alacritty_terminal/src/term/color.rs` (`Colors`, index layout).

use std::sync::{Arc, Mutex};

use alacritty_terminal::vte::ansi::Rgb;

/// Color-table index of the default foreground (`NamedColor::Foreground`).
pub const FOREGROUND_INDEX: usize = 256;
/// Color-table index of the default background (`NamedColor::Background`).
pub const BACKGROUND_INDEX: usize = 257;
/// Color-table index of the cursor color (`NamedColor::Cursor`).
pub const CURSOR_INDEX: usize = 258;

/// Closure that formats an OSC color reply for a resolved color. Produced by
/// alacritty's `Event::ColorRequest` (it already embeds the OSC prefix +
/// terminator, e.g. `\x1b]11;rgb:rrrr/gggg/bbbb\x07`).
pub type ColorFormatter = Arc<dyn Fn(Rgb) -> String + Send + Sync + 'static>;

/// A pending OSC 10/11/12 color *query* (the program asked with `?`) awaiting a
/// reply. Enqueued by the `EventListener` on `ColorRequest`, drained by the
/// backend read loop after each parse batch.
pub struct PendingColorQuery {
    /// Color-table index (256 = fg, 257 = bg, 258 = cursor).
    pub index: usize,
    /// Formats the reply escape sequence for a resolved color.
    pub format: ColorFormatter,
}

/// Thread-safe queue of pending color queries shared between the `EventListener`
/// (enqueues) and the backend read loop (drains + replies).
pub type SharedColorQueries = Arc<Mutex<Vec<PendingColorQuery>>>;

/// Create an empty shared color-query queue.
pub fn new_color_queries() -> SharedColorQueries {
    Arc::new(Mutex::new(Vec::new()))
}

/// Dynamic (OSC-set) colors read from the live `Term` color table for
/// rendering. `None` = not overridden → use the theme default.
///
/// - `foreground`/`background`/`cursor` — OSC 10/11/12.
/// - `indexed` — OSC 4 overrides for palette indices 0-255 (OSC 104 clears an
///   entry back to `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicColors {
    /// OSC 10 foreground override.
    pub foreground: Option<Rgb>,
    /// OSC 11 background override.
    pub background: Option<Rgb>,
    /// OSC 12 cursor override.
    pub cursor: Option<Rgb>,
    /// OSC 4 palette overrides for indices 0-255.
    pub indexed: [Option<Rgb>; 256],
}

impl Default for DynamicColors {
    fn default() -> Self {
        Self {
            foreground: None,
            background: None,
            cursor: None,
            indexed: [None; 256],
        }
    }
}

/// Pick the theme default color to report for a color-query `index` when the
/// program never set it via OSC. Handles the full color table:
/// - `0..=15` → `default_ansi[index]` (theme palette; `None` if not provided yet).
/// - `16..=255` → the fixed cube/grayscale color.
/// - `256`/`257`/`258` → foreground/background/cursor default.
///
/// Returns `None` when the corresponding default is unset (the caller then skips
/// the reply, matching alacritty's behavior for an unset cursor color).
pub fn default_color_for_index(
    index: usize,
    default_foreground: Option<Rgb>,
    default_background: Option<Rgb>,
    default_cursor: Option<Rgb>,
    default_ansi: Option<&[Rgb; 16]>,
) -> Option<Rgb> {
    match index {
        0..=15 => default_ansi.map(|ansi| ansi[index]),
        16..=255 => Some(crate::palette::extended_indexed_color(index as u8)),
        FOREGROUND_INDEX => default_foreground,
        BACKGROUND_INDEX => default_background,
        CURSOR_INDEX => default_cursor,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn default_color_maps_indices() {
        let fg = rgb(1, 2, 3);
        let bg = rgb(4, 5, 6);
        let cur = rgb(7, 8, 9);
        assert_eq!(
            default_color_for_index(FOREGROUND_INDEX, Some(fg), Some(bg), Some(cur), None),
            Some(fg)
        );
        assert_eq!(
            default_color_for_index(BACKGROUND_INDEX, Some(fg), Some(bg), Some(cur), None),
            Some(bg)
        );
        assert_eq!(
            default_color_for_index(CURSOR_INDEX, Some(fg), Some(bg), Some(cur), None),
            Some(cur)
        );
    }

    #[test]
    fn default_color_indexed_palette() {
        let ansi = [rgb(9, 9, 9); 16];
        // 0-15 → theme ANSI palette (when provided).
        assert_eq!(
            default_color_for_index(5, None, None, None, Some(&ansi)),
            Some(rgb(9, 9, 9))
        );
        // 0-15 without an ANSI palette yet → None (skip reply).
        assert_eq!(default_color_for_index(5, None, None, None, None), None);
        // 16-255 → fixed cube/grayscale (theme-independent).
        assert_eq!(
            default_color_for_index(16, None, None, None, None),
            Some(rgb(0, 0, 0))
        );
        assert_eq!(
            default_color_for_index(231, None, None, None, None),
            Some(rgb(255, 255, 255))
        );
        assert_eq!(
            default_color_for_index(232, None, None, None, None),
            Some(rgb(8, 8, 8))
        );
    }

    #[test]
    fn default_color_unknown_index_is_none() {
        assert_eq!(
            default_color_for_index(999, Some(rgb(1, 1, 1)), None, None, None),
            None
        );
    }

    #[test]
    fn default_color_unset_is_none() {
        // Unset cursor default → None → caller skips the reply.
        assert_eq!(
            default_color_for_index(
                CURSOR_INDEX,
                Some(rgb(1, 1, 1)),
                Some(rgb(2, 2, 2)),
                None,
                None
            ),
            None
        );
    }

    #[test]
    fn dynamic_colors_default_is_empty() {
        let dc = DynamicColors::default();
        assert!(dc.foreground.is_none());
        assert!(dc.background.is_none());
        assert!(dc.cursor.is_none());
        assert!(dc.indexed.iter().all(|c| c.is_none()));
    }
}
