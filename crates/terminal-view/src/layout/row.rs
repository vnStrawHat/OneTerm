//! Per-row layout computation.

use alacritty_terminal::term::cell::Flags;
use gpui::{FontStyle, FontWeight, TextRun};

use oneterm_highlight::{Class, Decoration};
use oneterm_terminal::{IndexedCell, is_default_background_color};

use super::super::box_drawing::block::is_full_width_band;
use super::super::box_drawing::drawing::{has_box_geometry, is_box_drawing};
use super::super::cell::{cell_colors, cell_style, is_blank};
use super::super::theme::TerminalTheme;
use super::types::{BatchedTextRun, BoxDrawCell, LayoutPoint, LayoutRect, RowLayout};

/// Lay out a single display row — build rects + text runs + box draws for the
/// cells on one line.
///
/// `cell_class` is the per-column semantic class (from the scanner + URL mask).
/// It replaces the old `url_mask: &[bool]` — `Class::Url` is one variant.
pub(crate) fn layout_row(
    line_cells: Vec<&IndexedCell>,
    display_line: i32,
    theme: &TerminalTheme,
    base_font: &gpui::Font,
    cell_class: &[u8],
) -> RowLayout {
    let mut rects: Vec<LayoutRect> = Vec::new();
    let mut runs: Vec<BatchedTextRun> = Vec::new();
    let mut box_draws: Vec<BoxDrawCell> = Vec::new();
    let mut current_batch: Option<BatchedTextRun> = None;
    let mut prev_had_extras = false;
    // Reusable scratch buffer for the box-geometry probe — cleared and refilled
    // per cell, but keeps its backing allocation across all cells in this row
    // (no per-cell `Vec` allocation on full-screen block workloads).
    let mut box_probe: Vec<(i32, i32, i32, i32)> = Vec::new();

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

        // ── Semantic class for this column ──
        let cls_byte = cell_class
            .get(point.column.0)
            .copied()
            .unwrap_or(Class::Default as u8);
        let class_style = theme.class_styles.style(cls_byte);

        let (fg, bg) = cell_colors(cell, theme, cls_byte);

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

        // ── Apply class decorations (additive) ──
        // Underline for Error/Warn/Url etc. — additive on top of ANSI fg.
        if class_style.deco == Decoration::Underline && style.underline.is_none() {
            style.underline = Some(gpui::UnderlineStyle {
                color: Some(style.color),
                thickness: gpui::px(1.0),
                wavy: false,
            });
        }

        // ── Apply class font style (additive OR, never removes) ──
        if class_style.font.bold {
            style.font.weight = FontWeight::BOLD;
        }
        if class_style.font.italic {
            style.font.style = FontStyle::Italic;
        }

        let zw = cell.zerowidth();

        if is_box_drawing(cell.c) && has_box_geometry(&mut box_probe, cell.c) {
            // Coalesce runs of identical full-width band glyphs (▀▄█…) sharing the
            // same colour into one stretched rect — the block analogue of the bg
            // run merge above. Partial-width/quadrant glyphs are never merged.
            let merged = if is_full_width_band(cell.c) {
                if let Some(last) = box_draws.last_mut() {
                    if last.c == cell.c
                        && last.color == style.color
                        && last.point.line == display_line
                        && last.point.column + last.num_cells as i32 == lp.column
                    {
                        last.num_cells += 1;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if !merged {
                box_draws.push(BoxDrawCell {
                    point: lp,
                    color: style.color,
                    c: cell.c,
                    num_cells: 1,
                });
            }
            // Flush the active text batch so the next real-text segment starts a
            // fresh run at its own absolute column. Do NOT emit a space-only run
            // for the block cell: each run is painted at an absolute column
            // origin (`cell_x(run.start.column)` in paint), so terminating the
            // run here keeps positioning correct. A space filler would instead
            // force one `shape_line` per block cell — the dominant cost on
            // full-screen block workloads (DOOM-fire), where the fire gradient
            // gives every cell a unique color so the spaces never batch.
            if let Some(old) = current_batch.take() {
                runs.push(old);
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
