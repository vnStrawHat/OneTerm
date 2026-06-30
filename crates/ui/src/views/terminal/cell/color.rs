//! Per-cell color resolution.

use std::mem;

use alacritty_terminal::term::cell::Cell;
use gpui::Hsla;

use oneterm_core::terminal::{is_app_chosen_exact_color, is_decorative_character};

use super::super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};

/// Convert cell → (fg Hsla, bg Hsla) sau inverse + contrast + dim.
pub(crate) fn cell_colors(cell: &Cell, theme: &TerminalTheme) -> (Hsla, Hsla) {
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
