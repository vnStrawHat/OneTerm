//! Selection highlight layout.

use std::collections::HashSet;

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

/// Build the set of (line, column) points in the selection → used to swap
/// fg/bg when drawing text.
pub(crate) fn build_selection_set(selection_rects: &[LayoutRect]) -> HashSet<LayoutPoint> {
    let mut set = HashSet::new();
    for r in selection_rects {
        for c in 0..r.num_cells {
            set.insert(LayoutPoint {
                line: r.point.line,
                column: r.point.column + c as i32,
            });
        }
    }
    set
}
