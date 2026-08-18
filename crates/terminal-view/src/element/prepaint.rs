//! `TerminalElement::prepaint` implementation.

use std::collections::VecDeque;

use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};
use gpui::{App, Bounds, Pixels, SharedString, Window, px};

use super::super::layout::{
    CursorPaint, GridMetrics, GutterCache, LayoutPoint, LayoutRect, LayoutState, RowCacheFrame,
    RowCacheStyle, is_blank, update_row_cache,
};
use super::super::theme::{TerminalTheme, resolve_cell_color};
use super::super::view::gutter_timestamps::SecondsOfDay;
use super::gutter::{GutterLayout, compute_gutter_width};
use super::measure::{FontMetrics, GridSizing};
use super::{gutter, measure};

impl super::TerminalElement {
    /// Prepaint the terminal element — compute layout state for paint.
    pub(crate) fn prepaint_terminal(
        &self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> LayoutState {
        let session = &self.session;
        let theme: &TerminalTheme = &self.theme;
        let font = &self.font;
        let font_size = self.font_size;
        let line_height_factor = self.line_height_factor;
        let cell_width_override = self.cell_width_override;
        let cursor_color_override = self.cursor_color_override;
        let cursor_shape_override = self.cursor_shape_override;
        let padding = self.padding;
        let show_gutter = self.show_gutter;
        let line_times: &VecDeque<SecondsOfDay> = &self.line_times;
        let line_time_base = self.line_time_base;
        let render_cache = &self.render_cache;
        let search_highlights = &self.search_highlights;
        let overlay = &self.overlay;

        let prepaint_start = std::time::Instant::now();
        let scale_factor = window.scale_factor().max(1.0);

        let FontMetrics {
            cell_width,
            line_height,
        } = measure::measure_font(
            font,
            font_size,
            line_height_factor,
            cell_width_override,
            window,
            cx,
        );

        let pad_left = px(padding.left);
        let pad_right = px(padding.right);
        let pad_top = px(padding.top);
        let pad_bottom = px(padding.bottom);

        // The view read `terminal_info()` once this frame and threads it in
        // (PERF-03); the gutter width needs `absolute_line_count`.
        let absolute_line_count = self.terminal_info.absolute_line_count;

        // ── Gutter width (cached) ──
        // Recompute only when num_digits *or* font *or* font_size changes, to avoid
        // gutter_width fluctuations that cause a resize loop with TUI apps.
        // When show_gutter = false, gutter_width = 0.
        let gutter_width = if show_gutter {
            let num_digits = gutter::gutter_digits(absolute_line_count);
            let cached = render_cache
                .borrow()
                .gutter
                .as_ref()
                .filter(|g| g.matches(num_digits, font_size, &font.family))
                .map(|g| g.width);
            match cached {
                Some(w) => w,
                None => {
                    // The borrow is released before `shape_line`.
                    let w = compute_gutter_width(num_digits, font, font_size, window);
                    render_cache.borrow_mut().gutter = Some(GutterCache {
                        width: w,
                        num_digits,
                        font_size,
                        font_family: font.family.clone(),
                    });
                    w
                }
            }
        } else {
            px(0.)
        };

        let (rows, cols) = measure::resize_session(
            session,
            bounds.size,
            &GridSizing {
                gutter_width,
                pad_left,
                pad_right,
                pad_top,
                pad_bottom,
                cell_width,
                line_height,
            },
            &mut render_cache.borrow_mut().grid_size,
            window,
            cx,
        );

        let snapshot_start = std::time::Instant::now();
        let snapshot = session.read(cx).snapshot();
        let snapshot_us = snapshot_start.elapsed().as_micros();
        let num_lines = snapshot.terminal_bounds.num_lines;
        let num_cols = snapshot.terminal_bounds.num_cols;
        let display_offset = snapshot.display_offset;

        let selection_rects = snapshot
            .selection
            .map(|sel| {
                super::super::layout::layout_selection(
                    &sel,
                    display_offset,
                    num_lines,
                    num_cols,
                    theme.selection,
                )
            })
            .unwrap_or_default();

        let cursor_display_line = snapshot.cursor.point.line.0 + display_offset as i32;

        let style_key = super::super::layout::RenderStyleKey {
            font: font.clone(),
            font_size_bits: f32::from(font_size).to_bits(),
            palette: theme.palette,
            min_contrast_bits: theme.min_contrast.to_bits(),
            semantic_enabled: overlay.is_enabled(),
            shell_profile: overlay.profile(),
        };

        update_row_cache(
            &mut render_cache.borrow_mut().rows,
            &RowCacheFrame {
                cells: &snapshot.cells,
                damage: &snapshot.damage,
                num_lines,
                display_offset,
                grid_size: (rows, cols),
                cursor_display_line,
            },
            &RowCacheStyle {
                theme,
                base_font: font,
                style_key: &style_key,
                overlay,
            },
        );

        // Fill the cached ShapedLine for runs not yet shaped.
        let mut shape_line_count: usize = 0;
        {
            let mut cache = render_cache.borrow_mut();
            let cache = &mut cache.rows;
            for i in 0..num_lines {
                let row = &mut cache.rows[i];
                if row.shaped_lines.len() != row.runs.len() {
                    row.shaped_lines.clear();
                    row.shaped_lines.reserve(row.runs.len());
                    for run in &row.runs {
                        // `SharedString::from(&str)` copies the text once into
                        // an `Arc<str>` (PERF-09: no intermediate `String`).
                        let shaped = window.text_system().shape_line(
                            SharedString::from(run.text.as_str()),
                            font_size,
                            std::slice::from_ref(&run.style),
                            Some(cell_width),
                        );
                        row.shaped_lines.push(Some(shaped));
                        shape_line_count += 1;
                    }
                }
            }
            cache.stats.shape_line_calls = shape_line_count;
            cache.stats.snapshot_us = snapshot_us;
            cache.stats.prepaint_us = prepaint_start.elapsed().as_micros();
        }

        let cursor = build_cursor(
            &snapshot,
            num_lines,
            num_cols,
            cursor_color_override,
            cursor_shape_override,
            theme,
        );

        let _hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);

        let gutter_bg = theme.gutter_bg;

        // Render the gutter up to the last line with actual content in the viewport,
        // but always at least 1 line. This preserves the UX at startup (only the first
        // few lines have output) without missing TUI content on lines below the cursor.
        let last_non_blank_display_line = snapshot
            .cells
            .iter()
            .rev()
            .find(|ic| !is_blank(&ic.cell))
            .map(|ic| ic.point.line.0 + display_offset as i32)
            .unwrap_or(0)
            .max(0);
        let gutter_line_count = ((last_non_blank_display_line as usize) + 1)
            .min(num_lines)
            .max(1);

        let gutter_entries = gutter::compute_gutter_entries(
            line_times,
            &GutterLayout {
                line_time_base,
                absolute_line_count,
                display_offset,
                viewport_lines: num_lines,
                max_entries: gutter_line_count,
            },
            bounds.origin,
            line_height,
            scale_factor,
        );

        let grid_origin = gpui::Point {
            x: px(measure::snap(
                f32::from(bounds.origin.x + gutter_width + pad_left),
                scale_factor,
            )),
            y: px(measure::snap(
                f32::from(bounds.origin.y + pad_top),
                scale_factor,
            )),
        };

        render_cache.borrow_mut().metrics = GridMetrics {
            bounds: Some(bounds),
            cell_width,
            line_height,
            grid_origin,
            rows: num_lines,
            cols: num_cols,
        };

        // ── Search highlight rects (display coordinates → LayoutRect) ──
        let search_rects: Vec<LayoutRect> = search_highlights
            .iter()
            .map(|h| LayoutRect {
                point: LayoutPoint {
                    line: h.display_line,
                    column: h.start_col,
                },
                num_cells: (h.end_col - h.start_col).max(0) as usize,
                color: if h.active {
                    theme.search_active
                } else {
                    theme.search_match
                },
            })
            .collect();

        LayoutState {
            selection_rects,
            search_rects,
            cursor,
            background: theme.bg,
            cell_width,
            line_height,
            grid_origin,
            gutter_width,
            gutter_entries,
            gutter_bg,
            num_lines,
        }
    }
}

fn build_cursor(
    snapshot: &oneterm_terminal::TerminalContent,
    num_lines: usize,
    num_cols: usize,
    cursor_color_override: Option<gpui::Hsla>,
    cursor_shape_override: oneterm_settings::TerminalCursorShape,
    theme: &TerminalTheme,
) -> Option<CursorPaint> {
    let c = &snapshot.cursor;
    if c.shape == CursorShape::Hidden {
        return None;
    }
    let display_line = c.point.line.0 + snapshot.display_offset as i32;
    if display_line < 0 || display_line >= num_lines as i32 {
        return None;
    }
    let col = c.point.column.0 as i32;
    let color = cursor_color_override.unwrap_or_else(|| {
        resolve_cell_color(
            &alacritty_terminal::vte::ansi::Color::Named(NamedColor::Cursor),
            theme,
        )
    });
    let shape = match cursor_shape_override {
        oneterm_settings::TerminalCursorShape::Block => CursorShape::Block,
        oneterm_settings::TerminalCursorShape::Bar => CursorShape::Beam,
        oneterm_settings::TerminalCursorShape::Underline => CursorShape::Underline,
    };
    // The glyph under the cursor, so a filled block can re-paint it on top of
    // the cursor quad instead of hiding it (CORR-43). Its colour is the cell's
    // background — the classic inverted-cell cursor.
    let glyph = snapshot
        .cells
        .get(display_line as usize * num_cols + col as usize)
        .filter(|ic| ic.point.column.0 == col as usize)
        .map(|ic| &ic.cell)
        .filter(|cell| !is_blank(cell) && cell.c != ' ')
        .map(|cell| {
            let (_, bg) = super::super::layout::cell_colors(cell, theme, 0);
            (cell.c, bg)
        });
    Some(CursorPaint {
        point: LayoutPoint {
            line: display_line,
            column: col,
        },
        color,
        shape,
        glyph,
    })
}
