//! Grid coordinate helpers.

use alacritty_terminal::selection::SelectionType;
use gpui::{Pixels, Point};

use super::LocalTerminalView;
use crate::views::terminal::element::GridMetrics;

impl LocalTerminalView {
    /// Convert a pixel position → (row, col) display coords (0-based from top of viewport).
    pub(crate) fn pixel_to_grid(metrics: &GridMetrics, pos: Point<Pixels>) -> Option<(f32, f32)> {
        if f32::from(metrics.cell_width) == 0.0 || f32::from(metrics.line_height) == 0.0 {
            return None;
        }
        // Subtract grid origin (includes gutter + pad_left + pad_top).
        let x = f32::from(pos.x - metrics.grid_origin.x);
        let y = f32::from(pos.y - metrics.grid_origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let col = x / f32::from(metrics.cell_width);
        let row = y / f32::from(metrics.line_height);
        // Reject clicks outside the grid bounds (right/bottom padding).
        if row >= metrics.rows as f32 || col >= metrics.cols as f32 {
            return None;
        }
        Some((row, col))
    }

    /// Selection type based on click count + alt.
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
