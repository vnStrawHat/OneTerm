//! `TerminalElement` — custom `gpui::Element` paint grid terminal từ
//! `TerminalContent` snapshot. #15: bg + cell rects (batched) + text runs
//! (batched) + cursor. Không giữ Entity — View (#16) truyền snapshot tươi ở
//! `render()`. Tham chiếu Zed `terminal_element.rs::layout_grid` + `paint`.
//!
//! Group A: cursor shape (Block/Bar/Underline), cursor blink, selection
//! inverse video (fg/bg swap cho cell trong selection).

use std::cell::RefCell;
use std::collections::HashSet;
use std::mem;
use std::rc::Rc;

use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{CursorShape, NamedColor};
use gpui::{
    App, Bounds, ContentMask, Element, ElementId, Entity, Font, FontStyle, FontWeight,
    GlobalElementId, Hsla, IntoElement, LayoutId, Pixels, Point as GpuiPoint,
    SharedString, TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, relative, size,
};

use myterm2_core::TerminalSession;
use myterm2_core::terminal::{
    IndexedCell, is_app_chosen_exact_color, is_decorative_character, is_default_background_color,
};

use super::terminal_view::LocalTerminalView;
use super::theme::{TerminalTheme, ensure_minimum_contrast, resolve_cell_color};

/// Metrics grid sau layout — View đọc để convert mouse pixel → (row,col).
/// Element ghi ở prepaint, View đọc ở handler (cùng thread → `Rc<RefCell>`).
#[derive(Clone, Copy, Default)]
pub(crate) struct GridMetrics {
    pub bounds: Option<Bounds<Pixels>>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Chiều rộng gutter (time + line number) bên trái terminal.
    pub gutter_width: Pixels,
}

/// Layout point (display line/col, 0-based từ top viewport).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
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

/// Một dòng gutter: text + vị trí pixel (top-left).
struct GutterEntry {
    text: SharedString,
    y: Pixels,
}

/// Thông tin layout computed ở prepaint → paint.
pub struct LayoutState {
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
    /// Chiều rộng gutter.
    gutter_width: Pixels,
    /// Mục gutter cho mỗi dòng hiển thị.
    gutter_entries: Vec<GutterEntry>,
    /// Màu text gutter.
    gutter_fg: Hsla,
    /// Màu nền gutter.
    gutter_bg: Hsla,
    /// Màu border (separator) — đồng bộ với Dock border.
    dock_border: Hsla,
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
    /// Per-line timestamps for gutter (0 = oldest line).
    line_times: Vec<String>,
    /// Border color từ GPUI theme (đồng bộ với Dock border).
    dock_border: Hsla,
    /// Offset trừ khỏi line number — accounts for phantom scrollback lines.
    line_number_offset: i32,
}

impl TerminalElement {
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
        dock_border: Hsla,
        line_number_offset: i32,
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
            line_times,
            dock_border,
            line_number_offset,
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

    /// Build set of (line, column) trong selection → để swap fg/bg khi vẽ text.
    fn build_selection_set(selection_rects: &[LayoutRect]) -> HashSet<LayoutPoint> {
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

    /// Build rects + text runs từ cells (theo display order). Trả (rects, runs).
    /// `selection_set` — nếu cell trong selection, swap fg/bg (inverse video).
    /// `hovered_url` — nếu cell trong URL range + Ctrl held, đổi fg → link color + underline.
    fn layout_grid(
        cells: &[IndexedCell],
        theme: &TerminalTheme,
        base_font: &Font,
        selection_set: &HashSet<LayoutPoint>,
        hovered_url: Option<&super::url::DetectedUrl>,
        ctrl_held: bool,
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

                // Kiểm tra cell có trong selection không.
                let lp = LayoutPoint {
                    line: display_line,
                    column: point.column.0 as i32,
                };
                // Kiểm tra cell có trong selection không — chỉ dùng cho blank check.
                let _is_selected = selection_set.contains(&lp);

                // Nền khác default → rect. Hoặc cell trong selection → rect nền.
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

                // Selection: giữ nguyên text color — selection background paint
                // riêng ở layer selection_rects (giống Zed, KHÔNG inverse video).
                let mut style = Self::cell_style(cell, fg, base_font);
                // Ctrl+hover URL highlight — đổi fg → link blue + underline.
                if ctrl_held {
                    if let Some(url) = hovered_url {
                        if url.row == display_line as usize
                            && point.column.0 >= url.start_col
                            && point.column.0 < url.end_col
                        {
                            style.color = gpui::hsla(0.6, 0.85, 0.65, 1.0);
                            style.underline = Some(UnderlineStyle {
                                color: Some(style.color),
                                thickness: px(1.0),
                                wavy: false,
                            });
                        }
                    }
                }
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

        // ── Gutter: [HH:MM:SS] line_number ──
        // Chiều rộng gutter = chiều rộng template text + padding.
        let gutter_template = "[00:00:00] 00000";
        let gutter_text_width = window
            .text_system()
            .shape_line(
                SharedString::from(gutter_template),
                font_px,
                &[TextRun {
                    len: gutter_template.len(),
                    color: gpui::black(),
                    background_color: None,
                    font: self.font.clone(),
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width();
        let gutter_width = gutter_text_width + px(8.0); // 4px padding mỗi bên
        // Padding trái cho terminal content — tránh text sát lề gutter separator.
        let content_padding = px(6.0);

        // Resize session theo bounds (race-free: chỉ khi đổi).
        // Trừ gutter_width + content_padding khỏi chiều rộng có sẵn.
        let grid_width = (f32::from(bounds.size.width) - f32::from(gutter_width) - f32::from(content_padding))
            .max(f32::from(cell_width));
        let cols = ((grid_width / f32::from(cell_width)).floor() as u16).max(1);
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

        // Selection highlight rects — tính trước để build selection_set
        // cho layout_grid (inverse video).
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

        let selection_set = Self::build_selection_set(&selection_rects);

        let (rects, runs) =
            Self::layout_grid(
                &snapshot.cells,
                &self.theme,
                &self.font,
                &selection_set,
                self.hovered_url.as_ref(),
                self.ctrl_held,
            );

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

        let _hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);

        // ── Gutter entries: [HH:MM:SS] line_number cho mỗi dòng hiển thị ──
        // Timestamp per-line: lấy từ line_times (tracked khi output mới).
        // Fallback "--:--:--" nếu chưa có data.
        let total_lines = snapshot.total_lines;
        let gutter_fg = {
            // Dim foreground cho gutter text.
            let fg = self.theme.fg;
            gpui::hsla(fg.h, fg.s, fg.l * 0.5, fg.a)
        };
        let gutter_bg = self.theme.bg;  // Cùng nền với terminal.
        let lt = &self.line_times;
        // Scan cells để tìm range content: từ dòng non-blank đầu tiên
        // đến dòng non-blank cuối cùng (hoặc cursor line). Blank lines
        // trong range này vẫn hiển thị gutter (vd dòng trống giữa ls output
        // và prompt tiếp theo).
        let mut first_content: Option<usize> = None;
        let mut last_content: Option<usize> = None;
        for ic in &snapshot.cells {
            let display_line = (ic.point.line.0 + display_offset as i32) as usize;
            if display_line >= num_lines {
                continue;
            }
            if !Self::is_blank(&ic.cell) {
                if first_content.is_none() {
                    first_content = Some(display_line);
                }
                last_content = Some(display_line);
            }
        }
        // Include cursor line in content range (cursor có thể trên dòng trống).
        let cursor_display = (snapshot.cursor.point.line.0 + display_offset as i32) as usize;
        if cursor_display < num_lines {
            if first_content.is_none() {
                first_content = Some(cursor_display);
            }
            last_content = Some(last_content.map_or(cursor_display, |l| l.max(cursor_display)));
        }
        // Mark all lines from first to last as having content.
        let mut has_content = vec![false; num_lines];
        if let (Some(first), Some(last)) = (first_content, last_content) {
            for i in first..=last.min(num_lines - 1) {
                has_content[i] = true;
            }
        }
        let gutter_entries: Vec<GutterEntry> = (0..num_lines)
            .map(|i| {
                if !has_content[i] {
                    // Dòng trống → gutter rỗng.
                    return GutterEntry {
                        text: SharedString::from(""),
                        y: bounds.origin.y + i as f32 * line_height,
                    };
                }
                // Line number 1-based, trừ phantom scrollback offset.
                // Fallback: nếu offset chưa init (-1), tính từ snapshot.
                let ln_offset = if self.line_number_offset < 0 {
                    (total_lines as i32 - num_lines as i32).max(0)
                } else {
                    self.line_number_offset
                };
                let line_num = total_lines as i32 - display_offset as i32 - num_lines as i32 + i as i32 + 1 - ln_offset;
                let line_num = line_num.max(1) as usize;
                // 0-based index into line_times (absolute grid position, NOT adjusted by offset).
                let abs_idx = (total_lines as i32 - display_offset as i32 - num_lines as i32 + i as i32).max(0) as usize;
                let time_str = if abs_idx < lt.len() {
                    lt[abs_idx].as_str()
                } else {
                    "--:--:--"
                };
                let text = format!("[{}] {:>5}", time_str, line_num);
                GutterEntry {
                    text: SharedString::from(text),
                    y: bounds.origin.y + i as f32 * line_height,
                }
            })
            .collect();

        // Grid origin = bên phải gutter + content_padding (left padding).
        let grid_origin = GpuiPoint {
            x: bounds.origin.x + gutter_width + content_padding,
            y: bounds.origin.y,
        };

        // Sink metrics cho View (mouse/wheel).
        // gutter_width trong metrics bao gồm content_padding để pixel_to_grid
        // convert chính xác từ tọa độ mouse.
        *self.metrics.borrow_mut() = GridMetrics {
            bounds: Some(bounds),
            cell_width,
            line_height,
            gutter_width: gutter_width + content_padding,
        };
        LayoutState {
            rects,
            selection_rects,
            runs,
            cursor,
            background: self.theme.bg,
            cell_width,
            line_height,
            grid_origin,
            gutter_width,
            gutter_entries,
            gutter_fg,
            gutter_bg,
            dock_border: self.dock_border,
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
            // Nền terminal.
            window.paint_quad(fill(bounds, layout.background));

            // ── Gutter: [HH:MM:SS] line_number ──
            let gw = layout.gutter_width;
            if gw > px(0.0) {
                // Nền gutter.
                let gutter_bounds = Bounds {
                    origin: bounds.origin,
                    size: size(gw, bounds.size.height),
                };
                window.paint_quad(fill(gutter_bounds, layout.gutter_bg));
                // Separator line (1px) giữa gutter và terminal.
                let sep_bounds = Bounds {
                    origin: GpuiPoint {
                        x: bounds.origin.x + gw - px(1.0),
                        y: bounds.origin.y,
                    },
                    size: size(px(1.0), bounds.size.height),
                };
                let sep_color = layout.dock_border;
                window.paint_quad(fill(sep_bounds, sep_color));
                // Gutter text cho mỗi dòng.
                let gfont_px = self.font_size;
                let glh = layout.line_height;
                for entry in &layout.gutter_entries {
                    let line = window.text_system().shape_line(
                        entry.text.clone(),
                        gfont_px,
                        std::slice::from_ref(&TextRun {
                            len: entry.text.len(),
                            color: layout.gutter_fg,
                            background_color: None,
                            font: self.font.clone(),
                            underline: None,
                            strikethrough: None,
                        }),
                        None,
                    );
                    let pos = GpuiPoint {
                        x: bounds.origin.x + px(4.0),
                        y: entry.y,
                    };
                    let _ = line.paint(pos, glh, TextAlign::Left, None, window, cx);
                }
            }

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

            // Cursor — vẽ theo shape (Block/Bar/Underline), có blink.
            if let Some(cur) = &layout.cursor {
                // Quyết định có vẽ cursor không:
                // - Không focus → luôn vẽ (để user thấy cursor ở đâu).
                // - Focus + blink on → chỉ vẽ khi cursor_visible.
                // - Focus + blink off → luôn vẽ.
                let should_paint = !self.focused || self.cursor_visible;
                if should_paint {
                    let pos = point(
                        (origin.x + cur.point.column as f32 * cw).floor(),
                        origin.y + cur.point.line as f32 * lh,
                    );
                    let sz = match cur.shape {
                        CursorShape::Beam => {
                            // Thanh dọc hẹp: 20% cell width, full height.
                            let bar_w = (cw * 0.2).max(px(1.0));
                            size(bar_w, lh)
                        }
                        CursorShape::Underline => {
                            // Gạch dưới: full width, 15% line height (min 2px).
                            let ul_h = (lh * 0.15).max(px(2.0));
                            size(cw, ul_h)
                        }
                        CursorShape::Block => {
                            // Block đầy: full cell.
                            size(cw, lh)
                        }
                        CursorShape::HollowBlock => {
                            // Hollow block: vẽ border (không fill) — fallback
                            // về block đầy cho đơn giản.
                            size(cw, lh)
                        }
                        CursorShape::Hidden => return,
                    };
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