//! `TerminalElement::paint` implementation.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, Bounds, ContentMask, ElementInputHandler, Entity, FocusHandle, Pixels, TextRun, Window,
    fill, point, px, size,
};

use oneterm_core::TerminalSession;

use super::super::box_drawing::{box_drawing_rects, rounded_corner_rects_aa};
use super::super::layout::{CursorPaint, LayoutState, RowLayoutCache};
use super::super::theme::TerminalTheme;
use super::super::view::LocalTerminalView;

/// Paint the terminal element from the prepainted `LayoutState`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_terminal(
    _session: &Entity<Box<dyn TerminalSession>>,
    view: &Entity<LocalTerminalView>,
    focus: &FocusHandle,
    theme: &TerminalTheme,
    font: &gpui::Font,
    font_size: Pixels,
    focused: bool,
    cursor_visible: bool,
    row_cache: &Rc<RefCell<RowLayoutCache>>,
    bounds: Bounds<Pixels>,
    layout: &mut LayoutState,
    window: &mut Window,
    cx: &mut App,
) {
    window.handle_input(focus, ElementInputHandler::new(bounds, view.clone()), cx);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut quad_count: usize = 0;
        let mut run_count: usize = 0;

        window.paint_quad(fill(bounds, layout.background));
        quad_count += 1;

        // ── Gutter ──
        let gw = layout.gutter_width;
        if gw > px(0.0) {
            let gutter_bounds = Bounds {
                origin: bounds.origin,
                size: size(gw, bounds.size.height),
            };
            window.paint_quad(fill(gutter_bounds, layout.gutter_bg));
            quad_count += 1;
            paint_gutter(window, theme, font, font_size, layout, bounds, &mut quad_count, cx);
        }

        let origin = layout.grid_origin;
        let cw = layout.cell_width;
        let lh = layout.line_height;

        let scale_factor = window.scale_factor().max(1.0);
        let origin_x_d = (f32::from(origin.x) * scale_factor).round() as i32;
        let origin_y_d = (f32::from(origin.y) * scale_factor).round() as i32;
        let cw_d = (f32::from(cw) * scale_factor).round() as i32;
        let lh_d = (f32::from(lh) * scale_factor).round() as i32;

        let cell_x = |col: i32| -> Pixels {
            px((origin_x_d + col * cw_d) as f32 / scale_factor)
        };
        let cell_y = |row: i32| -> Pixels {
            px((origin_y_d + row * lh_d) as f32 / scale_factor)
        };
        let run_w = |cells: usize| -> Pixels {
            px((cells as i32 * cw_d) as f32 / scale_factor)
        };
        let line_h_px = px(lh_d as f32 / scale_factor);

        let num_lines = layout.num_lines;
        let cache = row_cache.borrow();

        let mut bg_rect_count: usize = 0;
        for i in 0..num_lines {
            let y = cell_y(i as i32);
            for r in &cache.rows[i].rects {
                let pos = point(cell_x(r.point.column), y);
                let sz = size(run_w(r.num_cells), line_h_px);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
                quad_count += 1;
                bg_rect_count += 1;
            }
        }

        for r in &layout.selection_rects {
            let pos = point(cell_x(r.point.column), cell_y(r.point.line));
            let sz = size(run_w(r.num_cells), line_h_px);
            window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            quad_count += 1;
        }

        for i in 0..num_lines {
            let y = cell_y(i as i32);
            let row = &cache.rows[i];
            for (j, run) in row.runs.iter().enumerate() {
                if let Some(shaped) = row.shaped_lines.get(j).and_then(|s| s.as_ref()) {
                    let x = cell_x(run.start.column);
                    run.paint(shaped, x, y, cw, lh, window, cx);
                }
                run_count += 1;
            }
        }

        for i in 0..num_lines {
            let cell_y_logical = cell_y(i as i32);
            for bd in &cache.rows[i].box_draws {
                let cell_x_logical = cell_x(bd.point.column);
                if matches!(bd.c, '\u{256D}'..='\u{2570}') {
                    for (rx, ry, rw, rh, a) in rounded_corner_rects_aa(bd.c, cw_d, lh_d) {
                        let pos = point(
                            px(f32::from(cell_x_logical) + rx as f32 / scale_factor),
                            px(f32::from(cell_y_logical) + ry as f32 / scale_factor),
                        );
                        let sz = size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
                        let mut col = bd.color;
                        col.a *= a;
                        window.paint_quad(fill(Bounds::new(pos, sz), col));
                        quad_count += 1;
                    }
                    continue;
                }
                for (rx, ry, rw, rh) in box_drawing_rects(bd.c, cw_d, lh_d) {
                    let pos = point(
                        px(f32::from(cell_x_logical) + rx as f32 / scale_factor),
                        px(f32::from(cell_y_logical) + ry as f32 / scale_factor),
                    );
                    let sz = size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
                    window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
                    quad_count += 1;
                }
            }
        }
        drop(cache);

        if let Some(cur) = &layout.cursor {
            paint_cursor(cur, focused, cursor_visible, cw, lh, &cell_x, &cell_y, window, &mut quad_count);
        }

        {
            let mut cache = row_cache.borrow_mut();
            cache.stats.paint_quad_calls = quad_count;
            cache.stats.bg_rect_count = bg_rect_count;
            cache.stats.text_run_paints = run_count;
            cache.stats.frame_count += 1;
            if cache.stats.frame_count % 60 == 0 {
                eprintln!(
                    "[TerminalElement] frame={} lines={} dirty={} quads={} bg_rects={} shapes={} runs={} hashes={}",
                    cache.stats.frame_count,
                    cache.stats.total_lines,
                    cache.stats.dirty_lines,
                    cache.stats.paint_quad_calls,
                    cache.stats.bg_rect_count,
                    cache.stats.shape_line_calls,
                    cache.stats.text_run_paints,
                    cache.stats.hash_calls,
                );
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn paint_gutter(
    window: &mut Window,
    theme: &TerminalTheme,
    font: &gpui::Font,
    font_size: Pixels,
    layout: &LayoutState,
    bounds: Bounds<Pixels>,
    _quad_count: &mut usize,
    cx_gutter: &mut App,
) {
    let glh = layout.line_height;
    let clock_color = theme.clock_fg;
    let ln_color = theme.line_number_fg;
    for entry in &layout.gutter_entries {
        let runs: Vec<TextRun> = if entry.clock_len > 0 && entry.clock_len < entry.text.len() {
            vec![
                TextRun {
                    len: entry.clock_len,
                    color: clock_color,
                    background_color: None,
                    font: font.clone(),
                    underline: None,
                    strikethrough: None,
                },
                TextRun {
                    len: entry.text.len() - entry.clock_len,
                    color: ln_color,
                    background_color: None,
                    font: font.clone(),
                    underline: None,
                    strikethrough: None,
                },
            ]
        } else {
            vec![TextRun {
                len: entry.text.len(),
                color: clock_color,
                background_color: None,
                font: font.clone(),
                underline: None,
                strikethrough: None,
            }]
        };
        let line = window
            .text_system()
            .shape_line(entry.text.clone(), font_size, &runs, None);
        let pos = gpui::Point {
            x: bounds.origin.x + px(4.0),
            y: entry.y,
        };
        let _ = line.paint(pos, glh, gpui::TextAlign::Left, None, window, cx_gutter);
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cursor(
    cur: &CursorPaint,
    focused: bool,
    cursor_visible: bool,
    cw: Pixels,
    lh: Pixels,
    cell_x: &dyn Fn(i32) -> Pixels,
    cell_y: &dyn Fn(i32) -> Pixels,
    window: &mut Window,
    quad_count: &mut usize,
) {
    let should_paint = !focused || cursor_visible;
    if !should_paint {
        return;
    }
    let scale_factor = window.scale_factor().max(1.0);
    let pos = point(cell_x(cur.point.column), cell_y(cur.point.line));
    let sz = match cur.shape {
        CursorShape::Beam => {
            let bar_w = (cw * 0.2).max(px(1.0));
            let bar_w_d = (f32::from(bar_w) * scale_factor).ceil().max(1.0) as i32;
            size(
                px(bar_w_d as f32 / scale_factor),
                line_h_px(lh, scale_factor),
            )
        }
        CursorShape::Underline => {
            let ul_h = (lh * 0.15).max(px(2.0));
            let ul_h_d = (f32::from(ul_h) * scale_factor).ceil().max(2.0) as i32;
            size(
                run_w_px(cw, 1, scale_factor),
                px(ul_h_d as f32 / scale_factor),
            )
        }
        CursorShape::Block => size(run_w_px(cw, 1, scale_factor), line_h_px(lh, scale_factor)),
        CursorShape::HollowBlock => {
            size(run_w_px(cw, 1, scale_factor), line_h_px(lh, scale_factor))
        }
        CursorShape::Hidden => return,
    };
    window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
    *quad_count += 1;
}

fn run_w_px(cw: Pixels, cells: usize, scale_factor: f32) -> Pixels {
    let cw_d = (f32::from(cw) * scale_factor).round() as i32;
    px((cells as i32 * cw_d) as f32 / scale_factor)
}

fn line_h_px(lh: Pixels, scale_factor: f32) -> Pixels {
    let lh_d = (f32::from(lh) * scale_factor).round() as i32;
    px(lh_d as f32 / scale_factor)
}
