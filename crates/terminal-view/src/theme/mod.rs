//! Terminal theme: maps the gpui-component `Theme` → `TerminalPalette`, resolves
//! every frame `Color` to `gpui::Hsla` once per palette (`ColorTable`), applies
//! config / OSC colour overrides, and enforces minimum contrast.
//!
//! Pure utilities (no GPUI Element).

mod contrast;
mod input_channel;
mod palette;
mod terminal_theme;
#[cfg(test)]
mod tests;

pub(crate) use input_channel::{channel_chip, channel_color};
pub(crate) use terminal_theme::{
    TerminalTheme, apply_color_overrides, apply_dynamic_colors, build_terminal_theme,
};
