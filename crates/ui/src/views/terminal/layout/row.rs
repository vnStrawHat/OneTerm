//! Per-row layout computation.

use std::collections::HashSet;

use alacritty_terminal::term::cell::Flags;
use gpui::TextRun;

use oneterm_core::terminal::{IndexedCell, is_default_background_color};

use super::super::box_drawing::drawing::{box_drawing_rects, is_box_drawing, is_rounded_corner};
use super::super::cell::{cell_colors, cell_style, is_blank};
use super::super::theme::TerminalTheme;
use super::super::url::DetectedUrl;
use super::types::{BatchedTextRun, BoxDrawCell, LayoutPoint, LayoutRect, RowLayout};

/// Layout 1 display row — build rects + text runs + box draws cho cells
/// trên cùng 1 dòng.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_row(
    line_cells: Vec<&IndexedCell>,
    display_line: i32,
    theme: &TerminalTheme,
    base_font: &gpui::Font,
    selection_set: &HashSet<LayoutPoint>,
    hovered_url: Option<&DetectedUrl>,
    ctrl_held: bool,
) -> RowLayout {
    let _ = selection_set;
    let mut rects: Vec<LayoutRect> = Vec::new();
    let mut runs: Vec<BatchedTextRun> = Vec::new();
    let mut box_draws: Vec<BoxDrawCell> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;
    let mut prev_had_extras = false;

    for ic in line_cells {
        let point = ic.point;
        let cell = &ic.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        if cell.c == ' ' && prev_had_extras {
            prev_had_extras = false;
            continue;
        }
        prev_had_extras = matches!(cell.zerowidth(), Some(c) if !c.is_empty());

        let lp = LayoutPoint {
            line: display_line,
            column: point.column.0 as i32,
        };

        if is_blank(cell) {
            continue;
        }

        let (fg, bg) = cell_colors(cell, theme);

        if !is_default_background_color(&cell.bg) || cell.flags.contains(Flags::INVERSE) {
            let col = point.column.0 as i32;
            let merged = if let Some(last) = rects.last_mut() {
                if last.color == bg
                    && last.point.line == display_line
                    && last.point.column + last.num_cells as i32 == col
                {
                    last.num_cells += 1;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                rects.push(LayoutRect {
                    point: LayoutPoint {
                        line: display_line,
                        column: col,
                    },
                    num_cells: 1,
                    color: bg,
                });
            }
        }

        let mut style: TextRun = cell_style(cell, fg, base_font);
        if ctrl_held {
            if let Some(url) = hovered_url {
                if url.row == display_line as usize
                    && point.column.0 >= url.start_col
                    && point.column.0 < url.end_col
                {
                    style.color = gpui::hsla(0.6, 0.85, 0.65, 1.0);
                    style.underline = Some(gpui::UnderlineStyle {
                        color: Some(style.color),
                        thickness: gpui::px(1.0),
                        wavy: false,
                    });
                }
            }
        }
        let zw = cell.zerowidth();

        if is_box_drawing(cell.c)
            && (is_rounded_corner(cell.c) || !box_drawing_rects(cell.c, 16, 16).is_empty())
        {
            box_draws.push(BoxDrawCell {
                point: lp,
                color: style.color,
                c: cell.c,
            });
            let mut sp = style;
            sp.len = ' '.len_utf8();
            if let Some(b) = current_batch.as_mut() {
                if b.start.column + b.cell_count as i32 == lp.column && b.can_append(&sp) {
                    b.append_char(' ');
                } else {
                    let old = current_batch.take().unwrap();
                    runs.push(old);
                    current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
                }
            } else {
                current_batch = Some(BatchedTextRun::new(lp, ' ', sp));
            }
            continue;
        }

        if let Some(b) = current_batch.as_mut() {
            if b.can_append(&style)
                && b.start.line == lp.line
                && b.start.column + b.cell_count as i32 == lp.column
            {
                b.append_char(cell.c);
                if let Some(cs) = zw {
                    for &c in cs {
                        b.append_zw(c);
                    }
                }
            } else {
                let old = current_batch.take().unwrap();
                runs.push(old);
                let mut nb = BatchedTextRun::new(lp, cell.c, style);
                if let Some(cs) = zw {
                    for &c in cs {
                        nb.append_zw(c);
                    }
                }
                current_batch = Some(nb);
            }
        } else {
            let mut nb = BatchedTextRun::new(lp, cell.c, style);
            if let Some(cs) = zw {
                for &c in cs {
                    nb.append_zw(c);
                }
            }
            current_batch = Some(nb);
        }
    }
    if let Some(b) = current_batch {
        runs.push(b);
    }
    RowLayout {
        rects,
        runs,
        box_draws,
        shaped_lines: Vec::new(),
        prev_hash: 0,
    }
}
