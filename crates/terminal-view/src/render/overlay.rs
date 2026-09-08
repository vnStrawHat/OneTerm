//! Selection and search overlays: cell spans painted as translucent quads on
//! top of backgrounds and shapes, below glyphs. They never touch row plans.

use super::frame::{GridSize, Selection};

/// A span of columns on one display row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowSpan {
    pub row: u16,
    pub col: u16,
    pub cols: u16,
}

/// A search match in display coordinates, already viewport-clamped by the
/// view. `end_col` is exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchHighlight {
    pub display_line: i32,
    pub start_col: i32,
    pub end_col: i32,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SearchRect {
    pub span: RowSpan,
    pub active: bool,
}

/// Selection spans (parity item 36): block = same columns on every row; linear
/// = single row, or first row to EOL, full middle rows, last row from column 0.
pub(crate) fn selection_rects(sel: Selection, size: GridSize, out: &mut Vec<RowSpan>) {
    out.clear();
    let rows = i32::from(size.rows);
    let cols = size.cols;
    if cols == 0 || sel.end.row < sel.start.row {
        return;
    }
    let first = sel.start.row.max(0);
    let last = sel.end.row.min(rows - 1);
    if first > last {
        return;
    }
    let clamp = |c: u16| c.min(cols - 1);
    for row in first..=last {
        let (col, end) = if sel.block {
            let (a, b) = (clamp(sel.start.col), clamp(sel.end.col));
            (a.min(b), a.max(b))
        } else if sel.start.row == sel.end.row {
            (clamp(sel.start.col), clamp(sel.end.col))
        } else if row == sel.start.row {
            (clamp(sel.start.col), cols - 1)
        } else if row == sel.end.row {
            (0, clamp(sel.end.col))
        } else {
            (0, cols - 1)
        };
        out.push(RowSpan {
            row: row as u16,
            col,
            cols: end - col + 1,
        });
    }
}

/// Search spans, copied through with the `active` flag and clamped to the grid.
pub(crate) fn search_rects(
    highlights: &[SearchHighlight],
    size: GridSize,
    out: &mut Vec<SearchRect>,
) {
    out.clear();
    let rows = i32::from(size.rows);
    let cols = i32::from(size.cols);
    for h in highlights {
        if h.display_line < 0 || h.display_line >= rows {
            continue;
        }
        let start = h.start_col.clamp(0, cols);
        let end = h.end_col.clamp(0, cols);
        if end <= start {
            continue;
        }
        out.push(SearchRect {
            span: RowSpan {
                row: h.display_line as u16,
                col: start as u16,
                cols: (end - start) as u16,
            },
            active: h.active,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::frame::GridPoint;

    fn size() -> GridSize {
        GridSize { rows: 5, cols: 10 }
    }

    fn sel(start: (i32, u16), end: (i32, u16), block: bool) -> Selection {
        Selection {
            start: GridPoint {
                row: start.0,
                col: start.1,
            },
            end: GridPoint {
                row: end.0,
                col: end.1,
            },
            block,
        }
    }

    #[test]
    fn selection_block_and_linear_spans() {
        let mut out = Vec::new();
        selection_rects(sel((1, 2), (3, 4), true), size(), &mut out);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|s| s.col == 2 && s.cols == 3));
        assert_eq!(out[0].row, 1);

        selection_rects(sel((1, 7), (3, 2), false), size(), &mut out);
        assert_eq!(
            out,
            vec![
                RowSpan {
                    row: 1,
                    col: 7,
                    cols: 3
                },
                RowSpan {
                    row: 2,
                    col: 0,
                    cols: 10
                },
                RowSpan {
                    row: 3,
                    col: 0,
                    cols: 3
                },
            ]
        );

        selection_rects(sel((2, 3), (2, 5), false), size(), &mut out);
        assert_eq!(
            out,
            vec![RowSpan {
                row: 2,
                col: 3,
                cols: 3
            }]
        );
    }

    #[test]
    fn selection_rects_clamp_to_the_viewport() {
        let mut out = Vec::new();
        selection_rects(sel((-2, 4), (1, 30), false), size(), &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0],
            RowSpan {
                row: 0,
                col: 0,
                cols: 10
            }
        );
        assert_eq!(
            out[1],
            RowSpan {
                row: 1,
                col: 0,
                cols: 10
            }
        );
        selection_rects(sel((7, 0), (9, 0), false), size(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn search_rects_copy_through_with_active_flag() {
        let mut out = Vec::new();
        search_rects(
            &[
                SearchHighlight {
                    display_line: 1,
                    start_col: 2,
                    end_col: 5,
                    active: true,
                },
                SearchHighlight {
                    display_line: 9,
                    start_col: 0,
                    end_col: 3,
                    active: false,
                },
                SearchHighlight {
                    display_line: 2,
                    start_col: 8,
                    end_col: 40,
                    active: false,
                },
            ],
            size(),
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert!(out[0].active && out[0].span.cols == 3);
        assert_eq!(out[1].span.cols, 2, "clamped to the last column");
    }
}
