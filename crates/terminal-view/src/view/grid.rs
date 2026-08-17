//! Grid coordinate helpers: pixel → (row, col) and click-count → selection type.

use alacritty_terminal::selection::SelectionType;
use gpui::{Pixels, Point};

use crate::element::GridMetrics;

/// Convert a pixel position → (row, col) display coords (0-based from top of viewport).
///
/// Fractional: the mouse handlers pass the exact position through to the
/// backend, which decides the cell (and the side of a cell for selections).
/// `None` when the metrics are not measured yet or the point lies outside the
/// grid (gutter, padding, past the last row/column).
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

#[cfg(test)]
mod tests {
    use alacritty_terminal::selection::SelectionType;
    use gpui::{point, px};

    use super::{pixel_to_grid, sel_type};
    use crate::element::GridMetrics;

    fn metrics() -> GridMetrics {
        GridMetrics {
            bounds: None,
            cell_width: px(8.0),
            line_height: px(16.0),
            grid_origin: point(px(60.0), px(4.0)),
            rows: 24,
            cols: 80,
        }
    }

    #[test]
    fn maps_pixels_relative_to_the_grid_origin() {
        let (row, col) = pixel_to_grid(&metrics(), point(px(60.0 + 8.0 * 3.5), px(4.0 + 32.0)))
            .expect("inside the grid");
        assert_eq!(row, 2.0);
        assert_eq!(col, 3.5);
    }

    #[test]
    fn rejects_the_gutter_and_padding() {
        // Left of the grid origin (gutter) and above it.
        assert_eq!(pixel_to_grid(&metrics(), point(px(10.0), px(20.0))), None);
        assert_eq!(pixel_to_grid(&metrics(), point(px(70.0), px(1.0))), None);
        // Past the last column / row.
        assert_eq!(
            pixel_to_grid(&metrics(), point(px(60.0 + 8.0 * 80.0), px(20.0))),
            None
        );
        assert_eq!(
            pixel_to_grid(&metrics(), point(px(70.0), px(4.0 + 16.0 * 24.0))),
            None
        );
    }

    #[test]
    fn rejects_unmeasured_metrics() {
        assert_eq!(
            pixel_to_grid(&GridMetrics::default(), point(px(5.0), px(5.0))),
            None
        );
    }

    #[test]
    fn click_count_selects_word_and_line() {
        assert_eq!(sel_type(1, false), SelectionType::Simple);
        assert_eq!(sel_type(2, false), SelectionType::Semantic);
        assert_eq!(sel_type(3, false), SelectionType::Lines);
        assert_eq!(sel_type(5, false), SelectionType::Lines);
        assert_eq!(sel_type(2, true), SelectionType::Block);
    }
}
