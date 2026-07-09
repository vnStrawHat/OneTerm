//! `TerminalElement::prepaint` implementation.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};
use gpui::{App, Bounds, Pixels, SharedString, Window, px};

use oneterm_core::TerminalSession;

use super::super::cell::blank::is_blank;
use super::super::search::SearchHighlight;

use super::super::layout::{
    CursorPaint, GridMetrics, LayoutPoint, LayoutRect, LayoutState, RowLayoutCache,
    update_row_cache,
};
use super::super::theme::{TerminalTheme, resolve_cell_color};
use super::super::view::LocalTerminalView;
use super::gutter::compute_gutter_width;
use super::measure::FontMetrics;
use super::{gutter, measure};

/// Prepaint the terminal element — compute layout state for paint.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepaint_terminal(
    session: &gpui::Entity<Box<dyn TerminalSession>>,
    _view: &gpui::Entity<LocalTerminalView>,
    theme: &TerminalTheme,
    font: &gpui::Font,
    font_size: Pixels,
    line_height_factor: f32,
    cell_width_override: Option<f32>,
    cursor_color_override: Option<gpui::Hsla>,
    cursor_shape_override: crate::state::TerminalCursorShape,
    padding: crate::state::TerminalPadding,
    show_gutter: bool,
    line_times: &[String],
    line_time_base: usize,
    hovered_url: Option<&super::super::url::DetectedUrl>,
    ctrl_held: bool,
    cached_gutter: &Rc<RefCell<Option<(Pixels, usize)>>>,
    last_grid_size: &Rc<RefCell<Option<(u16, u16)>>>,
    metrics: &Rc<RefCell<GridMetrics>>,
    row_cache: &Rc<RefCell<RowLayoutCache>>,
    search_highlights: &[SearchHighlight],
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) -> LayoutState {
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

    // Read terminal_info early — need absolute_line_count for the gutter width.
    let info = session.read(cx).terminal_info();
    let absolute_line_count = info.absolute_line_count;

    // ── Gutter width (cached) ──
    // Recompute only when num_digits changes, to avoid gutter_width fluctuations
    // that cause a resize loop with TUI apps. When show_gutter = false, gutter_width = 0.
    let num_digits = absolute_line_count.max(1).to_string().len().max(2);
    let gutter_width = if show_gutter {
        let cg = cached_gutter.borrow_mut();
        if let Some((cached_w, cached_digits)) = *cg {
            if cached_digits == num_digits {
                cached_w
            } else {
                drop(cg); // release the borrow before calling shape_line
                let w = compute_gutter_width(
                    line_times,
                    absolute_line_count,
                    font,
                    font_size,
                    theme,
                    window,
                );
                *cached_gutter.borrow_mut() = Some((w, num_digits));
                w
            }
        } else {
            drop(cg);
            let w = compute_gutter_width(
                line_times,
                absolute_line_count,
                font,
                font_size,
                theme,
                window,
            );
            *cached_gutter.borrow_mut() = Some((w, num_digits));
            w
        }
    } else {
        px(0.)
    };

    let (rows, cols) = measure::resize_session(
        session,
        bounds.size,
        gutter_width,
        pad_left,
        pad_right,
        pad_top,
        pad_bottom,
        cell_width,
        line_height,
        last_grid_size,
        window,
        cx,
    );

    let snapshot = session.read(cx).snapshot();
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
    let selection_set = super::super::layout::build_selection_set(&selection_rects);

    let cursor_display_line = snapshot.cursor.point.line.0 + display_offset as i32;

    update_row_cache(
        &mut row_cache.borrow_mut(),
        &snapshot.cells,
        &snapshot.damage,
        num_lines,
        display_offset,
        (rows, cols),
        snapshot.selection,
        hovered_url,
        ctrl_held,
        theme,
        font,
        &selection_set,
        cursor_display_line,
    );

    // Fill the cached ShapedLine for runs not yet shaped.
    let mut shape_line_count: usize = 0;
    {
        let mut cache = row_cache.borrow_mut();
        for i in 0..num_lines {
            let row = &mut cache.rows[i];
            if row.shaped_lines.len() != row.runs.len() {
                row.shaped_lines.clear();
                row.shaped_lines.reserve(row.runs.len());
                for run in &row.runs {
                    let shaped = window.text_system().shape_line(
                        SharedString::from(run.text.clone()),
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
        cache.stats.prepaint_us = prepaint_start.elapsed().as_micros();
    }

    let cursor = build_cursor(
        &snapshot,
        num_lines,
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
        line_time_base,
        absolute_line_count,
        display_offset,
        num_lines,
        gutter_line_count,
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

    *metrics.borrow_mut() = GridMetrics {
        bounds: Some(bounds),
        cell_width,
        line_height,
        gutter_width: gutter_width + pad_left,
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

fn build_cursor(
    snapshot: &oneterm_core::terminal::TerminalContent,
    num_lines: usize,
    cursor_color_override: Option<gpui::Hsla>,
    cursor_shape_override: crate::state::TerminalCursorShape,
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
        crate::state::TerminalCursorShape::Block => CursorShape::Block,
        crate::state::TerminalCursorShape::Bar => CursorShape::Beam,
        crate::state::TerminalCursorShape::Underline => CursorShape::Underline,
    };
    Some(CursorPaint {
        point: LayoutPoint {
            line: display_line,
            column: col,
        },
        color,
        shape,
    })
}
