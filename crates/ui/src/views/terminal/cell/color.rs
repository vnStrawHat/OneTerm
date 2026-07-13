//! Per-cell color resolution.
//!
//! After ANSI/SGR resolution, applies the **semantic merge policy** (Layer 2):
//! class fg overrides only the *default* foreground (no SGR); explicit ANSI fg
//! is kept. Decorations and font styles are applied additively in `layout_row`.

use std::mem;

use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use gpui::Hsla;

use oneterm_core::terminal::{is_app_chosen_exact_color, is_decorative_character};

use super::super::highlight::to_gpui_hsla;
use super::super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};

/// Whether the cell's foreground is the terminal default (no explicit SGR fg).
fn is_default_foreground(fg: &Color) -> bool {
    matches!(
        fg,
        Color::Named(NamedColor::Foreground)
            | Color::Named(NamedColor::BrightForeground)
            | Color::Named(NamedColor::DimForeground)
    )
}

/// Convert cell → (fg Hsla, bg Hsla) after inverse + contrast + dim + semantic merge.
///
/// `class` is the semantic class byte (from `cell_class`). The merge policy:
/// - Default fg + non-Default class → class fg (the headline case).
/// - Explicit ANSI fg + non-Default class → keep ANSI fg (unless `override_ansi`).
/// - Default fg + Default class → theme fg.
pub(crate) fn cell_colors(cell: &Cell, theme: &TerminalTheme, class: u8) -> (Hsla, Hsla) {
    let mut fg = cell.fg;
    let mut bg = cell.bg;
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::INVERSE)
    {
        mem::swap(&mut fg, &mut bg);
    }
    let mut fg_h = resolve_cell_color(&fg, theme);
    let bg_h = resolve_cell_color(&bg, theme);

    // ── Semantic merge (Layer 2) ──
    let class_style = theme.class_styles.style(class);
    if let Some(class_fg) = class_style.fg {
        let is_default = is_default_foreground(&fg);
        if is_default || class_style.override_ansi {
            fg_h = to_gpui_hsla(class_fg);
        }
    }

    if !is_app_chosen_exact_color(&fg) && !is_decorative_character(cell.c) {
        fg_h = ensure_minimum_contrast(fg_h, bg_h, theme.min_contrast);
    }
    if cell
        .flags
        .contains(alacritty_terminal::term::cell::Flags::DIM)
    {
        fg_h.a *= 0.7;
    }
    (fg_h, bg_h)
}
