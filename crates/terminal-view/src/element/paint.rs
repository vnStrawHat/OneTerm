//! `TerminalElement::paint` implementation.

use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{
    App, Bounds, ContentMask, ElementInputHandler, Pixels, Point, TextRun, Window, fill, point, px,
    size,
};

use super::super::box_drawing::{box_drawing_rects_into, rounded_corner_rects_aa};
use super::super::layout::{CursorPaint, LayoutState};
use super::super::theme::TerminalTheme;

/// Device-snapped grid geometry for the paint pass. Converts (row, col) cell
/// coordinates into logical pixel positions/sizes, rounding to device pixels
/// once up front so every cell lands on the same pixel grid.
struct GridGeometry {
    scale_factor: f32,
    origin_x_d: i32,
    origin_y_d: i32,
    cw_d: i32,
    lh_d: i32,
    cell_width: Pixels,
    line_height: Pixels,
}

impl GridGeometry {
    fn new(
        origin: Point<Pixels>,
        cell_width: Pixels,
        line_height: Pixels,
        scale_factor: f32,
    ) -> Self {
        Self {
            scale_factor,
            origin_x_d: (f32::from(origin.x) * scale_factor).round() as i32,
            origin_y_d: (f32::from(origin.y) * scale_factor).round() as i32,
            cw_d: (f32::from(cell_width) * scale_factor).round() as i32,
            lh_d: (f32::from(line_height) * scale_factor).round() as i32,
            cell_width,
            line_height,
        }
    }

    fn cell_x(&self, col: i32) -> Pixels {
        px((self.origin_x_d + col * self.cw_d) as f32 / self.scale_factor)
    }

    fn cell_y(&self, row: i32) -> Pixels {
        px((self.origin_y_d + row * self.lh_d) as f32 / self.scale_factor)
    }

    fn run_w(&self, cells: usize) -> Pixels {
        px((cells as i32 * self.cw_d) as f32 / self.scale_factor)
    }

    fn line_h(&self) -> Pixels {
        px(self.lh_d as f32 / self.scale_factor)
    }
}

impl super::TerminalElement {
    /// Paint the terminal element from the prepainted `LayoutState`.
    pub(crate) fn paint_terminal(
        &self,
        bounds: Bounds<Pixels>,
        layout: &mut LayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = &self.view;
        let focus = &self.focus;
        let theme = &self.theme;
        let font = &self.font;
        let font_size = self.font_size;
        let focused = self.focused;
        let cursor_visible = self.cursor_visible;
        let row_cache = &self.render_cache.row_cache;

        window.handle_input(focus, ElementInputHandler::new(bounds, view.clone()), cx);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let paint_start = std::time::Instant::now();
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
            paint_gutter(window, theme, font, font_size, layout, bounds, cx);
        }

        let scale_factor = window.scale_factor().max(1.0);
        let geom = GridGeometry::new(
            layout.grid_origin,
            layout.cell_width,
            layout.line_height,
            scale_factor,
        );
        let lh = geom.line_height;
        let cw_d = geom.cw_d;
        let lh_d = geom.lh_d;
        let cell_x = |col: i32| geom.cell_x(col);
        let cell_y = |row: i32| geom.cell_y(row);
        let run_w = |cells: usize| geom.run_w(cells);
        let line_h_px = geom.line_h();

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

        // ── Search highlights (under the text, above cell backgrounds) ──
        for r in &layout.search_rects {
            let pos = point(cell_x(r.point.column), cell_y(r.point.line));
            let sz = size(run_w(r.num_cells), line_h_px);
            window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            quad_count += 1;
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
                    run.paint(shaped, x, y, lh, window, cx);
                }
                run_count += 1;
            }
        }

        // Reusable scratch buffer for box-draw geometry — one allocation for the
        // whole frame instead of one `Vec` per block cell (DOOM-fire draws a
        // primitive for nearly every cell on screen).
        let mut box_scratch: Vec<(i32, i32, i32, i32)> = Vec::new();
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
                box_drawing_rects_into(&mut box_scratch, bd.c, cw_d, lh_d);
                // Coalesced full-width band run: stretch the (full-width) rect
                // across `num_cells`. `num_cells > 1` only for `is_full_width_band`
                // glyphs (rx == 0, rw == cw_d), so widening rw is exact.
                let n = bd.num_cells.max(1) as i32;
                for &(rx, ry, rw, rh) in &box_scratch {
                    let pos = point(
                        px(f32::from(cell_x_logical) + rx as f32 / scale_factor),
                        px(f32::from(cell_y_logical) + ry as f32 / scale_factor),
                    );
                    let sz = size(px((rw * n) as f32 / scale_factor), px(rh as f32 / scale_factor));
                    window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
                    quad_count += 1;
                }
            }
        }
        drop(cache);

        if let Some(cur) = &layout.cursor {
            paint_cursor(cur, focused, cursor_visible, &geom, window, &mut quad_count);
        }

        {
            let mut cache = row_cache.borrow_mut();
            cache.stats.paint_quad_calls = quad_count;
            cache.stats.bg_rect_count = bg_rect_count;
            cache.stats.text_run_paints = run_count;
            cache.stats.paint_us = paint_start.elapsed().as_micros();
            cache.stats.frame_count += 1;
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            {
                let snapshot_us = cache.stats.snapshot_us;
                let frame_us = cache
                    .stats
                    .prepaint_us
                    .saturating_add(cache.stats.paint_us);
                cache.latency_samples.record(snapshot_us, frame_us);
            }
            #[cfg(feature = "terminal-diagnostics")]
            {
                let now = std::time::Instant::now();
                let should_report = cache.diagnostics_last_report.is_none_or(|last| {
                    now.duration_since(last) >= std::time::Duration::from_secs(5)
                });
                if should_report {
                    cache.diagnostics_last_report = Some(now);
                    log::debug!(
                        "[TerminalElement] frame={} lines={} dirty={} row_layouts={} quads={} bg_rects={} shapes={} runs={} hashes={} alloc_sites={} info_us={} snapshot_us={} prepaint_us={} paint_us={} | snapshot p95={}us p99={}us | frame p95={}us p99={}us samples={}",
                        cache.stats.frame_count,
                        cache.stats.total_lines,
                        cache.stats.dirty_lines,
                        cache.stats.row_layout_calls,
                        cache.stats.paint_quad_calls,
                        cache.stats.bg_rect_count,
                        cache.stats.shape_line_calls,
                        cache.stats.text_run_paints,
                        cache.stats.hash_calls,
                        cache.stats.allocation_buffer_sites,
                        cache.stats.terminal_info_us,
                        cache.stats.snapshot_us,
                        cache.stats.prepaint_us,
                        cache.stats.paint_us,
                        cache.latency_samples.snapshot_percentile(0.95),
                        cache.latency_samples.snapshot_percentile(0.99),
                        cache.latency_samples.frame_percentile(0.95),
                        cache.latency_samples.frame_percentile(0.99),
                        cache.latency_samples.len(),
                    );
                }
            }
        }
    });
    }
}

fn paint_gutter(
    window: &mut Window,
    theme: &TerminalTheme,
    font: &gpui::Font,
    font_size: Pixels,
    layout: &LayoutState,
    bounds: Bounds<Pixels>,
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

fn paint_cursor(
    cur: &CursorPaint,
    focused: bool,
    cursor_visible: bool,
    geom: &GridGeometry,
    window: &mut Window,
    quad_count: &mut usize,
) {
    let should_paint = !focused || cursor_visible;
    if !should_paint {
        return;
    }
    let scale_factor = geom.scale_factor;
    let cw = geom.cell_width;
    let lh = geom.line_height;
    let pos = point(geom.cell_x(cur.point.column), geom.cell_y(cur.point.line));
    let sz = match cur.shape {
        CursorShape::Beam => {
            let bar_w = (cw * 0.2).max(px(1.0));
            let bar_w_d = (f32::from(bar_w) * scale_factor).ceil().max(1.0) as i32;
            size(px(bar_w_d as f32 / scale_factor), geom.line_h())
        }
        CursorShape::Underline => {
            let ul_h = (lh * 0.15).max(px(2.0));
            let ul_h_d = (f32::from(ul_h) * scale_factor).ceil().max(2.0) as i32;
            size(geom.run_w(1), px(ul_h_d as f32 / scale_factor))
        }
        CursorShape::Block => size(geom.run_w(1), geom.line_h()),
        CursorShape::HollowBlock => size(geom.run_w(1), geom.line_h()),
        CursorShape::Hidden => return,
    };
    window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
    *quad_count += 1;
}
