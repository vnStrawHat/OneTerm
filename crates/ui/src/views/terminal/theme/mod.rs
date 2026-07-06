//! Terminal theme: maps the gpui-component `Theme` → `core::TerminalPalette`,
//! resolves `Color` → `gpui::Hsla`, and `ensure_minimum_contrast`.
//!
//! Pure utilities (no GPUI Element).

pub mod contrast;
pub mod palette;
pub mod resolve;
#[cfg(test)]
mod tests;

use gpui::Hsla;
use gpui_component::Theme;

use oneterm_core::terminal::TerminalPalette;

pub use contrast::{contrast_ratio, ensure_minimum_contrast};
pub use palette::{ANSI_16, hsla_from_vte, rgba_from_vte, vte_from_rgba};
pub use resolve::resolve_cell_color;

/// Terminal theme with a prebuilt palette + bg/fg (Hsla) + contrast threshold.
#[derive(Clone)]
pub struct TerminalTheme {
    pub palette: TerminalPalette,
    /// Default background (theme background) — for painting the element background.
    pub bg: Hsla,
    /// Default FG (theme foreground).
    pub fg: Hsla,
    /// Selection background color (highlights the selected text).
    pub selection: Hsla,
    /// Minimum contrast threshold (WCAG, default 4.5 ≈ AA).
    pub min_contrast: f32,
    /// Gutter text color (timestamp + line number). Default = dim fg (50% lightness).
    pub gutter_fg: Hsla,
    /// Gutter background color. Default = same as terminal background.
    pub gutter_bg: Hsla,
    /// Clock text color [HH:MM:SS]. Default = gutter_fg.
    pub clock_fg: Hsla,
    /// Line number text color. Default = gutter_fg.
    pub line_number_fg: Hsla,
    /// Search match highlight (non-active). Semi-transparent accent.
    pub search_match: Hsla,
    /// Active search match highlight (the current next/prev target).
    pub search_active: Hsla,
}

/// Build a `TerminalTheme` from the gpui-component active `Theme`.
pub fn build_terminal_theme(theme: &Theme) -> TerminalTheme {
    let c = &theme.colors;
    let fg = c.foreground;
    let bg = c.background;
    // cursor = caret color (fallback foreground).
    let cursor_rgba = if c.caret.a > 0.0 {
        c.caret.to_rgb()
    } else {
        c.foreground.to_rgb()
    };
    let palette = TerminalPalette {
        foreground: palette::vte_from_rgba(c.foreground.to_rgb()),
        background: palette::vte_from_rgba(c.background.to_rgb()),
        cursor: palette::vte_from_rgba(cursor_rgba),
        ansi: palette::ANSI_16,
        indexed: [None; 256],
    };
    TerminalTheme {
        palette,
        bg,
        fg,
        selection: if bg.l < 0.5 {
            gpui::hsla(0.589, 0.69, 0.165, 1.0) // #0d2847 dark blue
        } else {
            gpui::hsla(0.569, 0.92, 0.949, 1.0) // #e6f4fe light blue
        },
        min_contrast: 4.5,
        gutter_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        gutter_bg: bg,
        clock_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        line_number_fg: gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a),
        // Search highlights: yellow-ish, semi-transparent for non-active,
        // more opaque for the active match. Tuned for both light/dark backgrounds.
        search_match: gpui::hsla(0.13, 0.85, 0.5, 0.35),
        search_active: gpui::hsla(0.13, 0.9, 0.55, 0.7),
    }
}
