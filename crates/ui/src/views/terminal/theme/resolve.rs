//! Resolve alacritty `Color` → `Hsla` theo palette.

use alacritty_terminal::vte::ansi::Color;

use super::{TerminalTheme, hsla_from_vte};
use oneterm_core::terminal::resolve_color;

/// Resolve `Color` (alacritty) → `Hsla` theo palette.
pub fn resolve_cell_color(c: &Color, theme: &TerminalTheme) -> gpui::Hsla {
    hsla_from_vte(resolve_color(c, &theme.palette))
}
