//! `TerminalElement` — custom `gpui::Element` paint grid terminal từ
//! `TerminalContent` snapshot. #15: bg + cell rects (batched) + text runs
//! (batched) + cursor. Không giữ Entity — View (#16) truyền snapshot tươi ở
//! `render()`. Tham chiếu Zed `terminal_element.rs::layout_grid` + `paint`.

use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};
use gpui::{
    App, Bounds, ContentMask, Element, ElementId, Entity, Font, FontStyle, FontWeight,
    GlobalElementId, Hitbox, Hsla, IntoElement, LayoutId, Pixels, Point as GpuiPoint, SharedString,
    TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, relative, size,
};

use myterm2_core::TerminalSession;
use myterm2_core::terminal::{
    IndexedCell, is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};

use super::terminal_view::LocalTerminalView;
use super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};

/// Metrics grid sau layout — View đọc để convert mouse pixel → (row,col).
/// Element ghi ở `prepaint`, View đọc ở handler (cùng thread → `Rc<RefCell>`).
#[derive(Clone, Copy, Default)]
#[allow(dead_code)]
pub(crate) struct GridMetrics {
    pub bounds: Option<Bounds<Pixels>>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub display_offset: usize,
    pub num_lines: usize,
    pub num_cols: usize,
}

/// Layout point (display line/col, 0-based từ top viewport).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct LayoutPoint {
    line: i32,
    column: i32,
}

/// Rect nền (1 dòng, batch ngang).
#[derive(Clone, Debug)]
struct LayoutRect {
    point: LayoutPoint,
    num_cells: usize,
    color: Hsla,
}

/// Text run batch (các cell liên tiếp cùng style, cùng dòng).
struct BatchedTextRun {
    start: LayoutPoint,
    text: String,
    cell_count: usize,
    style: TextRun,
}

/// Thông tin layout computed ở prepaint → paint.
#[allow(dead_code)]
pub struct LayoutState {
    hitbox: Hitbox,
    rects: Vec<LayoutRect>,
    /// Selection highlight rects (painted between bg rects and text).
    selection_rects: Vec<LayoutRect>,
    runs: Vec<BatchedTextRun>,
    cursor: Option<CursorPaint>,
    background: Hsla,
    /// Pixel metrics.
    cell_width: Pixels,
    line_height: Pixels,
    /// Origin của grid (sau gutter/canh).
    grid_origin: GpuiPoint<Pixels>,
    num_lines: usize,
    num_cols: usize,
}

/// Con trỏ để paint.
struct CursorPaint {
    point: LayoutPoint,
    color: Hsla,
    shape: CursorShape,
}

/// Element paint terminal. Giữ `Entity<Box<dyn TerminalSession>>` để resize
/// trong prepaint (theo bounds) + snapshot tươi. View (#16) truyền entity
/// clone + theme + font.
pub(crate) struct TerminalElement {
    session: Entity<Box<dyn TerminalSession>>,
    theme: TerminalTheme,
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    focused: bool,
    /// Lần resize gần nhất (tránh resize lặp).
    last_size: Option<(u16, u16)>,
    /// Sink layout metrics cho View (mouse/wheel).
    metrics: Rc<RefCell<GridMetrics>>,
    /// View entity — để đăng ký IME input handler ở paint.
    view: Entity<LocalTerminalView>,
    /// Focus handle cho `handle_input`.
    focus: gpui::FocusHandle,
}

impl TerminalElement {
    pub(crate) fn new(
        session: Entity<Box<dyn TerminalSession>>,
        theme: TerminalTheme,
        font: Font,
        font_size: Pixels,
        line_height_factor: f32,
        focused: bool,
        metrics: Rc<RefCell<GridMetrics>>,
        view: Entity<LocalTerminalView>,
        focus: gpui::FocusHandle,
    ) -> Self {
        Self {
            session,
            theme,
            font,
            font_size,
            line_height_factor,
            focused,
            last_size: None,
            metrics,
            view,
            focus,
        }
    }

    /// Convert cell → (fg Hsla, bg Hsla) sau inverse + contrast + dim.
    fn cell_colors(cell: &Cell, theme: &TerminalTheme) -> (Hsla, Hsla) {
        let mut fg = cell.fg;
        let mut bg = cell.bg;
        if cell.flags.contains(Flags::INVERSE) {
            mem::swap(&mut fg, &mut bg);
        }
        let mut fg_h = resolve_cell_color(&fg, theme);
        let bg_h = resolve_cell_color(&bg, theme);
        if !is_app_chosen_exact_color(&fg) && !is_decorative_character(cell.c) {
            fg_h = ensure_minimum_contrast(fg_h, bg_h, theme.min_contrast);
        }
        if cell.flags.contains(Flags::DIM) {
            fg_h.a *= 0.7;
        }
        (fg_h, bg_h)
    }

    fn is_blank(cell: &Cell) -> bool {
        cell.c == ' '
            && is_default_background_color(&cell.bg)
            && cell.hyperlink().is_none()
            && !cell.flags.intersects(
                Flags::INVERSE | Flags::ALL_UNDERLINES | Flags::STRIKEOUT | Flags::WIDE_CHAR_SPACER,
            )
    }

    /// Build selection highlight rects từ `SelectionRange` (grid coords) →
    /// display coords. Mỗi dòng trong selection → 1 rect. Block selection →
    /// rect cột đều; Simple/Lines → full width (trừ dòng đầu/cuối).
    fn layout_selection(
        selection: &alacritty_terminal::selection::SelectionRange,
        display_offset: usize,
        num_lines: usize,
        num_cols: usize,
        color: Hsla,
    ) -> Vec<LayoutRect> {
        use alacritty_terminal::index::Line;

        // Convert grid line → display line.
        let to_display = |line: Line| -> i32 { line.0 + display_offset as i32 };
        let start_line = to_display(selection.start.line);
        let end_line = to_display(selection.end.line);

        // Bỏ qua nếu selection hoàn toàn ngoài viewport.
        if end_line < 0 || start_line >= num_lines as i32 {
            return Vec::new();
        }

        let clamped_start = start_line.max(0);
        let clamped_end = end_line.min(num_lines as i32 - 1);

        let mut rects = Vec::new();
        if selection.is_block {
            // Block: rect cột đều trên mỗi dòng.
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
            // Simple / Lines / Semantic: full width trừ dòng đầu & dòng cuối.
            for line in clamped_start..=clamped_end {
                let (col_start, num_cells) = if line == start_line && line == end_line {
                    // Cùng dòng: từ start column tới end column.
                    let s = selection.start.column.0 as i32;
                    let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                    (s, (e - s).max(0) as usize)
                } else if line == start_line {
                    // Dòng đầu: từ start column tới cuối.
                    let s = selection.start.column.0 as i32;
                    (s, (num_cols as i32 - s) as usize)
                } else if line == end_line {
                    // Dòng cuối: từ đầu tới end column.
                    let e = (selection.end.column.0 as i32 + 1).min(num_cols as i32);
                    (0, e as usize)
                } else {
                    // Dòng giữa: full width.
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

    /// Build rects + text runs từ cells (theo display order). Trả (rects, runs).
    fn layout_grid(
        cells: &[IndexedCell],
        theme: &TerminalTheme,
        base_font: &Font,
    ) -> (Vec<LayoutRect>, Vec<BatchedTextRun>) {
        use itertools::Itertools;
        let mut rects: Vec<LayoutRect> = Vec::new();
        let mut runs: Vec<BatchedTextRun> = Vec::new();
        let mut current_batch: Option<BatchedTextRun> = None;

        // Group cells theo grid line (display order), enumerate → display line.
        let linegroups = cells.iter().chunk_by(|ic| ic.point.line);
        for (line_index, (_, line)) in linegroups.into_iter().enumerate() {
            let display_line = line_index as i32;
            // Flush batch at line boundary.
            if let Some(b) = current_batch.take() {
                runs.push(b);
            }
            let mut prev_had_extras = false;
            for ic in line {
                let point = ic.point;
                let cell = &ic.cell;
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                // Skip space theo emoji variation sequence.
                if cell.c == ' ' && prev_had_extras {
                    prev_had_extras = false;
                    continue;
                }
                prev_had_extras = matches!(cell.zerowidth(), Some(c) if !c.is_empty());

                let (fg, bg) = Self::cell_colors(cell, theme);

                // Nền khác default → rect.
                if !is_default_background_color(&cell.bg) || cell.flags.contains(Flags::INVERSE) {
                    let col = point.column.0 as i32;
                    if let Some(last) = rects.last_mut() {
                        if last.color == bg
                            && last.point.line == display_line
                            && last.point.column + last.num_cells as i32 == col
                        {
                            last.num_cells += 1;
                            continue;
                        }
                    }
                    rects.push(LayoutRect {
                        point: LayoutPoint {
                            line: display_line,
                            column: col,
                        },
                        num_cells: 1,
                        color: bg,
                    });
                }

                if Self::is_blank(cell) {
                    continue;
                }
                let style = Self::cell_style(cell, fg, base_font);
                let lp = LayoutPoint {
                    line: display_line,
                    column: point.column.0 as i32,
                };
                let zw = cell.zerowidth();

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
        }
        if let Some(b) = current_batch {
            runs.push(b);
        }
        (rects, runs)
    }

    /// Build TextRun cho cell (bold/italic/underline/strikethrough).
    fn cell_style(cell: &Cell, fg: Hsla, base_font: &Font) -> TextRun {
        let underline = (cell.flags.intersects(Flags::ALL_UNDERLINES)
            || cell.hyperlink().is_some())
        .then(|| UnderlineStyle {
            color: Some(fg),
            thickness: px(1.0),
            wavy: cell.flags.contains(Flags::UNDERCURL),
        });
        let strikethrough =
            cell.flags
                .contains(Flags::STRIKEOUT)
                .then(|| gpui::StrikethroughStyle {
                    color: Some(fg),
                    thickness: px(1.0),
                });
        let weight = if cell.flags.contains(Flags::BOLD) {
            FontWeight::BOLD
        } else {
            base_font.weight
        };
        let style = if cell.flags.contains(Flags::ITALIC) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        TextRun {
            len: cell.c.len_utf8(),
            color: fg,
            background_color: None,
            font: Font {
                weight,
                style,
                ..base_font.clone()
            },
            underline,
            strikethrough,
        }
    }
}

impl BatchedTextRun {
    fn new(start: LayoutPoint, c: char, mut style: TextRun) -> Self {
        // `style.len` từ cell_style đã = c.len_utf8() → KHÔNG cộng thêm.
        let text = c.to_string();
        debug_assert_eq!(style.len, c.len_utf8());
        let _ = &mut style; // giữ style nguyên (len đã đúng)
        Self {
            start,
            text,
            cell_count: 1,
            style,
        }
    }
    fn can_append(&self, other: &TextRun) -> bool {
        self.style.font == other.font
            && self.style.color == other.color
            && self.style.background_color == other.background_color
            && self.style.underline == other.underline
            && self.style.strikethrough == other.strikethrough
    }
    fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_count += 1;
        self.style.len += c.len_utf8();
    }
    fn append_zw(&mut self, c: char) {
        self.text.push(c);
        self.style.len += c.len_utf8();
    }

    fn paint(
        &self,
        origin: GpuiPoint<Pixels>,
        cell_w: Pixels,
        line_h: Pixels,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = point(
            origin.x + self.start.column as f32 * cell_w,
            origin.y + self.start.line as f32 * line_h,
        );
        let line = window.text_system().shape_line(
            SharedString::from(self.text.clone()),
            font_size,
            std::slice::from_ref(&self.style),
            Some(cell_w),
        );
        let _ = line.paint(pos, line_h, TextAlign::Left, None, window, cx);
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
        // Font measure.
        let font_id = cx.text_system().resolve_font(&self.font);
        let font_px = self.font_size;
        let cell_width = cx
            .text_system()
            .advance(font_id, font_px, 'm')
            .map(|s| s.width)
            .unwrap_or(px(8.0));
        let line_height = px(f32::from(font_px) * self.line_height_factor);

        // Resize session theo bounds (race-free: chỉ khi đổi).
        let cols = ((f32::from(bounds.size.width) / f32::from(cell_width)).floor() as u16).max(1);
        let rows = ((f32::from(bounds.size.height) / f32::from(line_height)).floor() as u16).max(1);
        if self.last_size != Some((rows, cols)) {
            self.session.update(cx, |s, _| s.resize(rows, cols));
            self.last_size = Some((rows, cols));
        }

        // Snapshot tươi (sau resize grid).
        let snapshot = self.session.read(cx).snapshot();
        let num_lines = snapshot.terminal_bounds.num_lines;
        let num_cols = snapshot.terminal_bounds.num_cols;
        let display_offset = snapshot.display_offset;

        let (rects, runs) = Self::layout_grid(&snapshot.cells, &self.theme, &self.font);

        // Selection highlight rects.
        let selection_rects = snapshot
            .selection
            .map(|sel| {
                Self::layout_selection(
                    &sel,
                    display_offset,
                    num_lines,
                    num_cols,
                    self.theme.selection,
                )
            })
            .unwrap_or_default();

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
                    let color = resolve_cell_color(
                        &alacritty_terminal::vte::ansi::Color::Named(NamedColor::Cursor),
                        &self.theme,
                    );
                    Some(CursorPaint {
                        point: LayoutPoint {
                            line: display_line,
                            column: col,
                        },
                        color,
                        shape: c.shape,
                    })
                }
            }
        };

        let hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);
        // Sink metrics cho View (mouse/wheel).
        *self.metrics.borrow_mut() = GridMetrics {
            bounds: Some(bounds),
            cell_width,
            line_height,
            display_offset,
            num_lines,
            num_cols,
        };
        LayoutState {
            hitbox,
            rects,
            selection_rects,
            runs,
            cursor,
            background: self.theme.bg,
            cell_width,
            line_height,
            grid_origin: bounds.origin,
            num_lines,
            num_cols,
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
        // Đăng ký IME input handler (chỉ active khi focus).
        window.handle_input(
            &self.focus,
            gpui::ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            // Nền.
            window.paint_quad(fill(bounds, layout.background));
            let origin = layout.grid_origin;
            let cw = layout.cell_width;
            let lh = layout.line_height;
            let font_px = self.font_size;

            // Cell bg rects.
            for r in &layout.rects {
                let pos = point(
                    (origin.x + r.point.column as f32 * cw).floor(),
                    origin.y + r.point.line as f32 * lh,
                );
                let sz = size((cw * r.num_cells as f32).ceil(), lh);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            }

            // Selection highlight (sau bg rects, trước text để text hiện trên nền).
            for r in &layout.selection_rects {
                let pos = point(
                    (origin.x + r.point.column as f32 * cw).floor(),
                    origin.y + r.point.line as f32 * lh,
                );
                let sz = size((cw * r.num_cells as f32).ceil(), lh);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            }

            // Text runs.
            for run in &layout.runs {
                run.paint(origin, cw, lh, font_px, window, cx);
            }

            // Cursor (block, filled khi focused).
            if let Some(cur) = &layout.cursor {
                if self.focused || matches!(cur.shape, CursorShape::Block) {
                    let pos = point(
                        (origin.x + cur.point.column as f32 * cw).floor(),
                        origin.y + cur.point.line as f32 * lh,
                    );
                    let sz = size(cw, lh);
                    window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
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
