//! OSC dynamic-colour support (OSC 4/10/11/12 set/query + OSC 104/110/111/112
//! reset).
//!
//! The engine applies a *set* to its own override table and reports a *query*
//! as `VtEvent::ColorQuery { key, terminator }`, leaving the answer to the
//! embedder because only the embedder owns the theme fallback. This module
//! holds the two things that needs:
//!
//! - **Render** OSC-set colours — [`DynamicColors`], read from the engine's
//!   override table by [`crate::model::TerminalModel::dynamic_colors`].
//! - **Answer queries** — a [`PendingColorQuery`] per `?`, queued by the router
//!   and answered by [`crate::TerminalPump::color_replies`] off the live engine
//!   colours, falling back to [`default_color_for_key`].
//!
//! The colour index space is [`ColorKey`] (`US-0082`). The 256 / 257 / 258
//! constants the fork used for foreground / background / cursor are **deleted**:
//! they were three magic numbers three files had to agree on, and the typed key
//! makes the agreement the compiler's problem. `Rgb` is still the legacy value
//! type here because `crates/terminal-view`'s `theme/palette.rs` passes and
//! reads it (`US-0085`).

use std::sync::Arc;

use alacritty_terminal::vte::ansi::Rgb;
use oneterm_vt::ColorKey;

use crate::backend::DefaultColors;
use crate::engine_shim::legacy_rgb;

/// Closure that formats an OSC colour reply for a resolved colour. Built by the
/// router from the query's own OSC prefix and string terminator, e.g.
/// `\x1b]11;rgb:rrrr/gggg/bbbb\x07`.
pub type ColorFormatter = Arc<dyn Fn(Rgb) -> String + Send + Sync + 'static>;

/// A pending OSC 4/10/11/12 colour *query* (the program asked with `?`) awaiting
/// a reply. Enqueued by the router during a parse batch, answered by the pump
/// once the engine's colours can be read.
pub struct PendingColorQuery {
    /// Which colour was asked for.
    pub key: ColorKey,
    /// Formats the reply escape sequence for a resolved colour.
    pub format: ColorFormatter,
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

/// Pick the theme default colour to report for a colour query the program never
/// set via OSC:
/// - `Palette(0..=15)` → `defaults.ansi[index]` (`None` until the UI sets it).
/// - `Palette(16..=255)` → the fixed cube/greyscale colour.
/// - `Foreground` / `Background` / `Cursor` → the theme default.
///
/// Returns `None` when the corresponding default is unset (the caller then skips
/// the reply, which is what the engine being replaced did for an unset cursor
/// colour).
pub fn default_color_for_key(key: ColorKey, defaults: &DefaultColors) -> Option<Rgb> {
    match key {
        ColorKey::Palette(index @ 0..=15) => defaults
            .ansi
            .map(|ansi| legacy_rgb(ansi[usize::from(index)])),
        ColorKey::Palette(index) => Some(crate::palette::extended_indexed_color(index)),
        ColorKey::Foreground | ColorKey::BrightForeground => defaults.foreground.map(legacy_rgb),
        ColorKey::Background => defaults.background.map(legacy_rgb),
        ColorKey::Cursor => defaults.cursor.map(legacy_rgb),
        // No sequence the engine accepts queries a dim slot, but the key space
        // covers them, so they answer with the colour they are derived from
        // rather than falling through to "no reply".
        ColorKey::DimForeground => defaults.foreground.map(legacy_rgb),
        ColorKey::Dim(index) => defaults
            .ansi
            .map(|ansi| legacy_rgb(ansi[usize::from(index & 7)])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_vt::Rgb as VtRgb;

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    fn vt(r: u8, g: u8, b: u8) -> VtRgb {
        VtRgb { r, g, b }
    }

    fn defaults() -> DefaultColors {
        DefaultColors {
            foreground: Some(vt(1, 2, 3)),
            background: Some(vt(4, 5, 6)),
            cursor: Some(vt(7, 8, 9)),
            ansi: None,
        }
    }

    #[test]
    fn default_color_maps_the_three_dynamic_keys() {
        let defaults = defaults();
        assert_eq!(
            default_color_for_key(ColorKey::Foreground, &defaults),
            Some(rgb(1, 2, 3))
        );
        assert_eq!(
            default_color_for_key(ColorKey::Background, &defaults),
            Some(rgb(4, 5, 6))
        );
        assert_eq!(
            default_color_for_key(ColorKey::Cursor, &defaults),
            Some(rgb(7, 8, 9))
        );
    }

    #[test]
    fn default_color_indexed_palette() {
        let mut defaults = DefaultColors::default();
        // 0-15 → theme ANSI palette (when provided).
        defaults.ansi = Some([vt(9, 9, 9); 16]);
        assert_eq!(
            default_color_for_key(ColorKey::Palette(5), &defaults),
            Some(rgb(9, 9, 9))
        );
        // 0-15 without an ANSI palette yet → None (skip the reply).
        defaults.ansi = None;
        assert_eq!(default_color_for_key(ColorKey::Palette(5), &defaults), None);
        // 16-255 → the fixed cube / greyscale ramp (theme-independent).
        assert_eq!(
            default_color_for_key(ColorKey::Palette(16), &defaults),
            Some(rgb(0, 0, 0))
        );
        assert_eq!(
            default_color_for_key(ColorKey::Palette(231), &defaults),
            Some(rgb(255, 255, 255))
        );
        assert_eq!(
            default_color_for_key(ColorKey::Palette(232), &defaults),
            Some(rgb(8, 8, 8))
        );
    }

    /// The typed key is what replaced the 256 / 257 / 258 constants: every key
    /// the engine can report has to land somewhere, and an unset default still
    /// means "skip the reply" rather than "reply black".
    #[test]
    fn every_color_key_is_answered_or_skipped() {
        let empty = DefaultColors::default();
        for key in [
            ColorKey::Foreground,
            ColorKey::Background,
            ColorKey::Cursor,
            ColorKey::BrightForeground,
            ColorKey::DimForeground,
            ColorKey::Dim(3),
            ColorKey::Palette(0),
        ] {
            assert_eq!(
                default_color_for_key(key, &empty),
                None,
                "{key:?} has no theme default yet"
            );
        }
        // Only the fixed part of the palette answers without a theme.
        assert!(default_color_for_key(ColorKey::Palette(200), &empty).is_some());
    }

    #[test]
    fn default_color_unset_is_none() {
        // Unset cursor default → None → the caller skips the reply.
        let defaults = DefaultColors {
            foreground: Some(vt(1, 1, 1)),
            background: Some(vt(2, 2, 2)),
            cursor: None,
            ansi: None,
        };
        assert_eq!(default_color_for_key(ColorKey::Cursor, &defaults), None);
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
