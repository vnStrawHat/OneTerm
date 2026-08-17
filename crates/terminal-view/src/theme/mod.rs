//! Terminal theme: maps the gpui-component `Theme` → `core::TerminalPalette`,
//! resolves `Color` → `gpui::Hsla`, applies config / OSC colour overrides, and
//! `ensure_minimum_contrast`.
//!
//! Pure utilities (no GPUI Element).

mod contrast;
mod palette;
mod terminal_theme;
#[cfg(test)]
mod tests;

pub(crate) use contrast::ensure_minimum_contrast;
pub(crate) use palette::resolve_cell_color;
pub(crate) use terminal_theme::{
    TerminalTheme, apply_color_overrides, apply_dynamic_colors, build_terminal_theme,
};
