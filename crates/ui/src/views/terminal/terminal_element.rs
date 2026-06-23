//! `TerminalElement` — custom `gpui::Element` paint grid terminal từ
//! `TerminalContent` snapshot. Orchestration: dùng `terminal_element_layout`,
//! `terminal_element_cell`, `terminal_element_box` cho chi tiết render.
//!
//! Không giữ Entity — View truyền snapshot tươi ở `render()`.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};
use gpui::{
    App, Bounds, ContentMask, Element, ElementId, Entity, Font, GlobalElementId, Hsla, IntoElement,
    LayoutId, Pixels, SharedString, TextRun, Window, fill, point, px, relative, size,
};

use myterm2_core::TerminalSession;

use super::terminal_element_box::{box_drawing_rects, rounded_corner_rects_aa};
pub(crate) use super::terminal_element_layout::{
    GridMetrics, LayoutState, RowLayoutCache, update_row_cache,
};
use super::terminal_view::LocalTerminalView;
use super::theme::{TerminalTheme, resolve_cell_color};

/// Element paint terminal. Giữ `Entity<Box<dyn TerminalSession>>` để resize
/// trong prepaint (theo bounds) + snapshot tươi. View truyền entity
/// clone + theme + font.
pub(crate) struct TerminalElement {
    session: Entity<Box<dyn TerminalSession>>,
    theme: TerminalTheme,
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    focused: bool,
    /// Có vẽ cursor không (blink logic: true = hiện, false = ẩn giữa blink).
    cursor_visible: bool,
    /// Lần resize gần nhất (tránh resize lặp).
    last_size: Option<(u16, u16)>,
    /// Sink layout metrics cho View (mouse/wheel).
    metrics: Rc<RefCell<GridMetrics>>,
    /// View entity — để đăng ký IME input handler ở paint.
    view: Entity<LocalTerminalView>,
    /// Focus handle cho `handle_input`.
    focus: gpui::FocusHandle,
    /// URL đang hover (Ctrl held) — highlight cells trong range.
    hovered_url: Option<super::url::DetectedUrl>,
    /// Ctrl đang held.
    ctrl_held: bool,
    /// Padding quanh terminal content (top/right/bottom/left px).
    padding: crate::state::TerminalPadding,
    /// Cell width override (None = auto từ font advance).
    cell_width_override: Option<f32>,
    /// Cursor color override (None = theme caret).
    cursor_color_override: Option<Hsla>,
    /// Cursor shape override từ config (Block/Bar/Underline).
    /// Override snapshot shape từ shell (trừ Hidden) — giống Windows Terminal.
    cursor_shape_override: crate::state::TerminalCursorShape,
    /// Per-line timestamps for gutter (0 = oldest line).
    line_times: Vec<String>,
    /// Per-row layout cache — skip recompute cho non-dirty rows.
    row_cache: Rc<RefCell<RowLayoutCache>>,
}

impl TerminalElement {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session: Entity<Box<dyn TerminalSession>>,
        theme: TerminalTheme,
        font: Font,
        font_size: Pixels,
        line_height_factor: f32,
        focused: bool,
        cursor_visible: bool,
        metrics: Rc<RefCell<GridMetrics>>,
        view: Entity<LocalTerminalView>,
        focus: gpui::FocusHandle,
        hovered_url: Option<super::url::DetectedUrl>,
        ctrl_held: bool,
        line_times: Vec<String>,
        padding: crate::state::TerminalPadding,
        cell_width_override: Option<f32>,
        cursor_color_override: Option<Hsla>,
        cursor_shape_override: crate::state::TerminalCursorShape,
        row_cache: Rc<RefCell<RowLayoutCache>>,
    ) -> Self {
        Self {
            session,
            theme,
            font,
            font_size,
            line_height_factor,
            focused,
            cursor_visible,
            last_size: None,
            metrics,
            view,
            focus,
            hovered_url,
            ctrl_held,
            padding,
            cell_width_override,
            cursor_color_override,
            cursor_shape_override,
            line_times,
            row_cache,
        }
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = LayoutState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Fill parent (panel cho size); prepaint tính rows/cols từ bounds thật.
        let mut style = gpui::Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        let id = window.request_layout(style, None, cx);
        (id, ())
    }

    fn prepaint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // ── Device-pixel snapping ──
        let scale_factor = window.scale_factor().max(1.0);
        let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

        // Font measure.
        let font_id = cx.text_system().resolve_font(&self.font);
        let font_px = self.font_size;
        let cell_width = if let Some(cw) = self.cell_width_override {
            px(snap_px(cw))
        } else {
            let raw = cx
                .text_system()
                .ch_advance(font_id, font_px)
                .map(|s| f32::from(s))
                .unwrap_or_else(|_| {
                    cx.text_system()
                        .advance(font_id, font_px, 'm')
                        .map(|s| f32::from(s.width))
                        .unwrap_or(8.0)
                });
            px(snap_px(raw))
        };
        let font_ascent = cx.text_system().ascent(font_id, font_px);
        let font_descent = cx.text_system().descent(font_id, font_px);
        let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
        let factor_height = f32::from(font_px) * self.line_height_factor;
        let line_height = px(snap_px(factor_height.max(natural_line_height)));

        // ── Padding ──
        let pad_left = px(self.padding.left);
        let pad_right = px(self.padding.right);
        let pad_top = px(self.padding.top);
        let pad_bottom = px(self.padding.bottom);

        // ── Gutter width ──
        let num_digits = self.line_times.len().max(1).to_string().len().max(2);
        let gutter_template = format!("[00:00:00] {}", "0".repeat(num_digits));
        let gutter_text_width = window
            .text_system()
            .shape_line(
                SharedString::from(gutter_template),
                font_px,
                &[TextRun {
                    len: "[00:00:00] ".len() + num_digits,
                    color: gpui::black(),
                    background_color: None,
                    font: self.font.clone(),
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width();
        let gutter_width = gutter_text_width + px(8.0);

        // Resize session theo bounds.
        let grid_width = (f32::from(bounds.size.width)
            - f32::from(gutter_width)
            - f32::from(pad_left)
            - f32::from(pad_right))
        .max(f32::from(cell_width));
        let grid_width_device = (grid_width * scale_factor).floor().max(1.0);
        let cell_width_device = f32::from(cell_width) * scale_factor;
        let cols = ((grid_width_device / cell_width_device).floor() as u16).max(1);
        let avail_height =
            f32::from(bounds.size.height) - f32::from(pad_top) - f32::from(pad_bottom);
        let avail_height_device = (avail_height * scale_factor).floor().max(0.0);
        let line_height_device = f32::from(line_height) * scale_factor;
        let rows = ((avail_height_device / line_height_device).floor() as u16).max(1);
        if self.last_size != Some((rows, cols)) {
            self.session.update(cx, |s, _| s.resize(rows, cols));
            self.last_size = Some((rows, cols));
        }

        // Snapshot tươi.
        let snapshot = self.session.read(cx).snapshot();
        let num_lines = snapshot.terminal_bounds.num_lines;
        let num_cols = snapshot.terminal_bounds.num_cols;
        let display_offset = snapshot.display_offset;
        let total_lines = snapshot.total_lines;

        // Selection.
        let selection_rects = snapshot
            .selection
            .map(|sel| {
                super::terminal_element_layout::layout_selection(
                    &sel,
                    display_offset,
                    num_lines,
                    num_cols,
                    self.theme.selection,
                )
            })
            .unwrap_or_default();
        let selection_set = super::terminal_element_layout::build_selection_set(&selection_rects);

        let cursor_display_line = snapshot.cursor.point.line.0 + display_offset as i32;

        // Update row cache.
        update_row_cache(
            &mut self.row_cache.borrow_mut(),
            &snapshot.cells,
            &snapshot.damage,
            num_lines,
            display_offset,
            (rows, cols),
            snapshot.selection,
            self.hovered_url.as_ref(),
            self.ctrl_held,
            &self.theme,
            &self.font,
            &selection_set,
            cursor_display_line,
        );

        // Fill cached ShapedLine cho runs chưa được shape.
        let mut shape_line_count: usize = 0;
        {
            let mut cache = self.row_cache.borrow_mut();
            for i in 0..num_lines {
                let row = &mut cache.rows[i];
                if row.shaped_lines.len() != row.runs.len() {
                    row.shaped_lines.clear();
                    row.shaped_lines.reserve(row.runs.len());
                    for run in &row.runs {
                        let shaped = window.text_system().shape_line(
                            SharedString::from(run.text.clone()),
                            font_px,
                            std::slice::from_ref(&run.style),
                            Some(cell_width),
                        );
                        row.shaped_lines.push(Some(shaped));
                        shape_line_count += 1;
                    }
                }
            }
            cache.stats.shape_line_calls = shape_line_count;
        }

        // Cursor.
        let cursor = {
            let c = &snapshot.cursor;
            if c.shape == CursorShape::Hidden {
                None
            } else {
                let display_line = c.point.line.0 + snapshot.display_offset as i32;
                if display_line < 0 || display_line >= num_lines as i32 {
                    None
                } else {
                    let col = c.point.column.0 as i32;
                    let color = self.cursor_color_override.unwrap_or_else(|| {
                        resolve_cell_color(
                            &alacritty_terminal::vte::ansi::Color::Named(NamedColor::Cursor),
                            &self.theme,
                        )
                    });
                    let shape = match self.cursor_shape_override {
                        crate::state::TerminalCursorShape::Block => CursorShape::Block,
                        crate::state::TerminalCursorShape::Bar => CursorShape::Beam,
                        crate::state::TerminalCursorShape::Underline => CursorShape::Underline,
                    };
                    Some(super::terminal_element_layout::CursorPaint {
                        point: super::terminal_element_layout::LayoutPoint {
                            line: display_line,
                            column: col,
                        },
                        color,
                        shape,
                    })
                }
            }
        };

        let _hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);

        // Gutter entries.
        let gutter_bg = self.theme.gutter_bg;
        let lt = &self.line_times;
        let mut first_content: Option<usize> = None;
        let mut last_content: Option<usize> = None;
        for ic in &snapshot.cells {
            let display_line = (ic.point.line.0 + display_offset as i32) as usize;
            if display_line >= num_lines {
                continue;
            }
            if !super::terminal_element_cell::is_blank(&ic.cell) {
                if first_content.is_none() {
                    first_content = Some(display_line);
                }
                last_content = Some(display_line);
            }
        }
        let cursor_display = (snapshot.cursor.point.line.0 + display_offset as i32) as usize;
        if cursor_display < num_lines {
            if first_content.is_none() {
                first_content = Some(cursor_display);
            }
            last_content = Some(last_content.map_or(cursor_display, |l| l.max(cursor_display)));
        }
        let mut has_content = vec![false; num_lines];
        if let (Some(first), Some(last)) = (first_content, last_content) {
            for i in first..=last.min(num_lines - 1) {
                has_content[i] = true;
            }
        }
        let gutter_entries = (0..num_lines)
            .map(|i| {
                if !has_content[i] {
                    return super::terminal_element_layout::GutterEntry {
                        text: SharedString::from(""),
                        clock_len: 0,
                        y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
                    };
                }
                let line_num =
                    total_lines as i32 - display_offset as i32 - num_lines as i32 + i as i32 + 1;
                let line_num = line_num.max(1) as usize;
                let abs_idx = (total_lines as i32 - display_offset as i32 - num_lines as i32
                    + i as i32)
                    .max(0) as usize;
                let time_str = if abs_idx < lt.len() {
                    lt[abs_idx].as_str()
                } else {
                    "--:--:--"
                };
                let text = format!("[{}] {:>width$}", time_str, line_num, width = num_digits);
                let clock_len = 1 + time_str.len() + 2;
                super::terminal_element_layout::GutterEntry {
                    text: SharedString::from(text),
                    clock_len,
                    y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
                }
            })
            .collect();

        // Grid origin.
        let grid_origin = gpui::Point {
            x: px(snap_px(f32::from(
                bounds.origin.x + gutter_width + pad_left,
            ))),
            y: px(snap_px(f32::from(bounds.origin.y + pad_top))),
        };

        // Sink metrics cho View.
        *self.metrics.borrow_mut() = GridMetrics {
            bounds: Some(bounds),
            cell_width,
            line_height,
            gutter_width: gutter_width + pad_left,
        };
        LayoutState {
            selection_rects,
            cursor,
            background: self.theme.bg,
            cell_width,
            line_height,
            grid_origin,
            gutter_width,
            gutter_entries,
            gutter_bg,
            num_lines,
        }
    }

    fn paint(
        &mut self,
        _global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.handle_input(
            &self.focus,
            gpui::ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            let mut quad_count: usize = 0;
            let mut run_count: usize = 0;

            // Nền terminal.
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
                let gfont_px = self.font_size;
                let glh = layout.line_height;
                let clock_color = self.theme.clock_fg;
                let ln_color = self.theme.line_number_fg;
                for entry in &layout.gutter_entries {
                    let runs: Vec<TextRun> =
                        if entry.clock_len > 0 && entry.clock_len < entry.text.len() {
                            vec![
                                TextRun {
                                    len: entry.clock_len,
                                    color: clock_color,
                                    background_color: None,
                                    font: self.font.clone(),
                                    underline: None,
                                    strikethrough: None,
                                },
                                TextRun {
                                    len: entry.text.len() - entry.clock_len,
                                    color: ln_color,
                                    background_color: None,
                                    font: self.font.clone(),
                                    underline: None,
                                    strikethrough: None,
                                },
                            ]
                        } else {
                            vec![TextRun {
                                len: entry.text.len(),
                                color: clock_color,
                                background_color: None,
                                font: self.font.clone(),
                                underline: None,
                                strikethrough: None,
                            }]
                        };
                    let line = window
                        .text_system()
                        .shape_line(entry.text.clone(), gfont_px, &runs, None);
                    let pos = gpui::Point {
                        x: bounds.origin.x + px(4.0),
                        y: entry.y,
                    };
                    let _ = line.paint(pos, glh, gpui::TextAlign::Left, None, window, cx);
                }
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
            let cache = self.row_cache.borrow();

            // Cell bg rects.
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

            // Selection highlight.
            for r in &layout.selection_rects {
                let pos = point(cell_x(r.point.column), cell_y(r.point.line));
                let sz = size(run_w(r.num_cells), line_h_px);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
                quad_count += 1;
            }

            // Text runs.
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

            // Box-drawing primitive.
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
                            let sz =
                                size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
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

            // Cursor.
            if let Some(cur) = &layout.cursor {
                let should_paint = !self.focused || self.cursor_visible;
                if should_paint {
                    let pos = point(cell_x(cur.point.column), cell_y(cur.point.line));
                    let sz = match cur.shape {
                        CursorShape::Beam => {
                            let bar_w = (cw * 0.2).max(px(1.0));
                            let bar_w_d = (f32::from(bar_w) * scale_factor).ceil().max(1.0) as i32;
                            size(px(bar_w_d as f32 / scale_factor), line_h_px)
                        }
                        CursorShape::Underline => {
                            let ul_h = (lh * 0.15).max(px(2.0));
                            let ul_h_d = (f32::from(ul_h) * scale_factor).ceil().max(2.0) as i32;
                            size(run_w(1), px(ul_h_d as f32 / scale_factor))
                        }
                        CursorShape::Block => size(run_w(1), line_h_px),
                        CursorShape::HollowBlock => size(run_w(1), line_h_px),
                        CursorShape::Hidden => return,
                    };
                    window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
                    quad_count += 1;
                }
            }

            // Stats.
            {
                let mut cache = self.row_cache.borrow_mut();
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
}

impl IntoElement for TerminalElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}
