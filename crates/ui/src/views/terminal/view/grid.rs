//! Grid coordinate helpers.

use alacritty_terminal::selection::SelectionType;
use gpui::{Pixels, Point};

use super::LocalTerminalView;
use crate::views::terminal::element::GridMetrics;

impl LocalTerminalView {
    /// Convert pixel position → (row, col) display (0-based từ top viewport).
    pub(crate) fn pixel_to_grid(metrics: &GridMetrics, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let b = metrics.bounds?;
        if f32::from(metrics.cell_width) == 0.0 || f32::from(metrics.line_height) == 0.0 {
            return None;
        }
        let x = f32::from(pos.x - b.origin.x - metrics.gutter_width);
        let y = f32::from(pos.y - b.origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let col = x / f32::from(metrics.cell_width);
        let row = y / f32::from(metrics.line_height);
        Some((row, col))
    }

    /// Selection type theo click count + alt.
    pub(crate) fn sel_type(click_count: usize, alt: bool) -> SelectionType {
        if alt {
            SelectionType::Block
        } else {
            match click_count {
                2 => SelectionType::Semantic,
                n if n >= 3 => SelectionType::Lines,
                _ => SelectionType::Simple,
            }
        }
    }
}
