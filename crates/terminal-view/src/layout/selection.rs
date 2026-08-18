//! Selection highlight layout.

use gpui::Hsla;

use super::types::{LayoutPoint, LayoutRect};

/// Build selection highlight rects from `SelectionRange` (grid coords) →
/// display coords. Each line in the selection → one rect. Block selection →
/// even column rects; Simple/Lines → full width (except first/last line).
pub(crate) fn layout_selection(
    selection: &alacritty_terminal::selection::SelectionRange,
    display_offset: usize,
    num_lines: usize,
    num_cols: usize,
    color: Hsla,
) -> Vec<LayoutRect> {
    use alacritty_terminal::index::Line;

    let to_display = |line: Line| -> i32 { line.0 + display_offset as i32 };
    let start_line = to_display(selection.start.line);
    let end_line = to_display(selection.end.line);

    if end_line < 0 || start_line >= num_lines as i32 {
        return Vec::new();
    }

    let clamped_start = start_line.max(0);
    let clamped_end = end_line.min(num_lines as i32 - 1);

    let mut rects = Vec::new();
    if selection.is_block {
        let start_col = selection.start.column.0 as i32;
        let end_col = (selection.end.column.0 as i32).min(num_cols as i32 - 1);
        if end_col < start_col {
            return Vec::new();
        }
        for line in clamped_start..=clamped_end {
            rects.push(LayoutRect {
                point: LayoutPoint {
                    line,
                    column: start_col,
                },
                num_cells: (end_col - start_col + 1) as usize,
                color,
            });
        }
    } else {
        for line in clamped_start..=clamped_end {
            let (col_start, num_cells) = if line == start_line && line == end_line {
                let s = selection.start.column.0 as i32;
                let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                (s, (e - s).max(0) as usize)
            } else if line == start_line {
                let s = selection.start.column.0 as i32;
                (s, (num_cols as i32 - s) as usize)
            } else if line == end_line {
                let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                (0, e as usize)
            } else {
                (0, num_cols)
            };
            if num_cells > 0 {
                rects.push(LayoutRect {
                    point: LayoutPoint {
                        line,
                        column: col_start,
                    },
                    num_cells,
                    color,
                });
            }
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::selection::SelectionRange;
    use gpui::Hsla;

    use super::layout_selection;

    fn range(start: (i32, usize), end: (i32, usize), is_block: bool) -> SelectionRange {
        SelectionRange::new(
            Point::new(Line(start.0), Column(start.1)),
            Point::new(Line(end.0), Column(end.1)),
            is_block,
        )
    }

    fn spans(rects: &[super::LayoutRect]) -> Vec<(i32, i32, usize)> {
        rects
            .iter()
            .map(|r| (r.point.line, r.point.column, r.num_cells))
            .collect()
    }

    #[test]
    fn single_line_selection_covers_start_to_end_inclusive() {
        let rects = layout_selection(&range((2, 3), (2, 6), false), 0, 24, 80, Hsla::default());
        assert_eq!(spans(&rects), vec![(2, 3, 4)]);
    }

    #[test]
    fn multi_line_selection_fills_middle_lines() {
        let rects = layout_selection(&range((1, 5), (3, 2), false), 0, 24, 80, Hsla::default());
        // First line from the start column to the end, middle full width,
        // last line from column 0 through the end column.
        assert_eq!(spans(&rects), vec![(1, 5, 75), (2, 0, 80), (3, 0, 3)]);
    }

    #[test]
    fn block_selection_uses_the_same_columns_on_every_line() {
        let rects = layout_selection(&range((0, 4), (2, 7), true), 0, 24, 80, Hsla::default());
        assert_eq!(spans(&rects), vec![(0, 4, 4), (1, 4, 4), (2, 4, 4)]);
    }

    #[test]
    fn selection_is_shifted_by_the_display_offset_and_clipped() {
        // Grid lines −2..=1 with the viewport scrolled up by 1: display rows
        // −1..=2 → only rows 0..=2 are emitted.
        let rects = layout_selection(&range((-2, 0), (1, 9), false), 1, 3, 80, Hsla::default());
        assert_eq!(spans(&rects), vec![(0, 0, 80), (1, 0, 80), (2, 0, 10)]);
    }

    #[test]
    fn selection_outside_the_viewport_is_empty() {
        assert!(
            layout_selection(&range((-10, 0), (-5, 3), false), 0, 24, 80, Hsla::default())
                .is_empty()
        );
        assert!(
            layout_selection(&range((30, 0), (31, 3), false), 0, 24, 80, Hsla::default())
                .is_empty()
        );
    }
}
