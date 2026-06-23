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
    GlobalElementId, Hsla, IntoElement, LayoutId, Pixels, Point as GpuiPoint, ShapedLine,
    SharedString, TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, relative, size,
};

use myterm2_core::TerminalSession;
use myterm2_core::terminal::{
    IndexedCell, TermDamageInfo, is_app_chosen_exact_color, is_decorative_character,
    is_default_background_color,
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

/// Một dòng gutter: text + vị trí pixel (top-left) + byte length của phần clock.
struct GutterEntry {
    text: SharedString,
    /// Byte length của phần clock "[HH:MM:SS] " (không bao gồm line number).
    clock_len: usize,
    y: Pixels,
}

/// Thông tin layout computed ở prepaint → paint.
/// Per-row artifacts (rects, runs, box_draws) lives trong `RowLayoutCache`
/// (Rc<RefCell>) — paint đọc từ đó, không clone. Giống AtlasEngine `_p.rows`.
pub struct LayoutState {
    selection_rects: Vec<LayoutRect>,
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
    /// Màu nền gutter.
    gutter_bg: Hsla,
    /// Số display lines (để paint biết iterate bao nhiêu row trong cache).
    num_lines: usize,
}

/// Con trỏ để paint.
struct CursorPaint {
    point: LayoutPoint,
    color: Hsla,
    shape: CursorShape,
}

/// Một cell box-drawing (U+2500–U+257F) sẽ được vẽ bằng primitive fill
/// thay vì rasterize font glyph → pixel-perfect, không anti-alias blur.
struct BoxDrawCell {
    point: LayoutPoint,
    color: Hsla,
    c: char,
}

/// Layout artifacts cho 1 display row — cached qua frames để skip recompute.
/// Giống AtlasEngine `ShapedRow` (dirtyTop/dirtyBottom + glyphIndices/Advances).
struct RowLayout {
    rects: Vec<LayoutRect>,
    runs: Vec<BatchedTextRun>,
    box_draws: Vec<BoxDrawCell>,
    /// Cached `ShapedLine` cho mỗi `BatchedTextRun` — parallel vec với `runs`.
    /// None = chưa shape (mới recompute), Some = đã shape (reuse qua frames).
    /// Giống AtlasEngine `ShapedRow.glyphIndices` — glyph data persisted,
    /// chỉ re-rasterize khi row dirty.
    shaped_lines: Vec<Option<ShapedLine>>,
    /// Content hash của dòng ở frame trước — dùng để detect thay đổi
    /// mà Term::damage() không track (vd input()/write_at_cursor()
    /// không gọi damage_line()).
    prev_hash: u64,
}

impl RowLayout {
    fn empty() -> Self {
        Self {
            rects: Vec::new(),
            runs: Vec::new(),
            box_draws: Vec::new(),
            shaped_lines: Vec::new(),
            prev_hash: 0,
        }
    }
}

/// Thống kê render 1 frame — đếm paint calls để đo bottleneck.
/// Log mỗi 60 frame (~1s) qua eprintln.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct FrameStats {
    /// Tổng số display lines trong viewport.
    pub total_lines: usize,
    /// Số lines thực sự recompute layout (dirty).
    pub dirty_lines: usize,
    /// Số `paint_quad` calls trong paint().
    pub paint_quad_calls: usize,
    /// Số `shape_line` calls trong prepaint() — chỉ cho dirty rows.
    /// Non-dirty rows reuse cached ShapedLine → không gọi shape_line.
    pub shape_line_calls: usize,
    /// Số text runs painted trong paint() — tất cả dùng cached ShapedLine.
    pub text_run_paints: usize,
    /// Số background rects sau khi coalesce adjacent same-color cells.
    /// AtlasEngine: 1 quad per contiguous same-color run thay vì 1 per cell.
    pub bg_rect_count: usize,
    /// Số `line_hash` calls trong update_row_cache — chỉ cursor line
    /// thay vì tất cả non-dirty rows.
    pub hash_calls: usize,
    /// Frame counter — log mỗi 60 frame.
    pub frame_count: u64,
}

/// Per-row layout cache — persisted qua frames qua `Rc<RefCell>`.
/// Giống AtlasEngine `_p.rows` (Vec<ShapedRow*>) + `_p.colorBitmapGenerations`.
///
/// Chỉ recompute layout cho dirty rows (từ `TermDamageInfo`).
/// Non-dirty rows reuse cached `RowLayout` — skip cell iteration + color
/// resolution + text batching.
///
/// Invalidate toàn bộ khi:
/// - Grid size đổi (resize) — `prev_grid_size` mismatch.
/// - `display_offset` đổi (scroll) — damage thường đã là `Full`.
/// - Selection / hover URL / Ctrl state đổi — affect per-cell styling.
pub(crate) struct RowLayoutCache {
    /// Per-row layout artifacts, indexed by display line (0 = top viewport).
    rows: Vec<RowLayout>,
    /// Previous grid size — detect resize.
    prev_grid_size: Option<(u16, u16)>,
    /// Previous display_offset — detect scroll.
    prev_display_offset: usize,
    /// Previous selection — detect change (affects inverse video).
    prev_selection: Option<alacritty_terminal::selection::SelectionRange>,
    /// Previous hovered URL — detect change (affects link highlight).
    prev_hovered_url: Option<super::url::DetectedUrl>,
    /// Previous Ctrl held — detect change.
    prev_ctrl_held: bool,
    /// Frame stats — updated mỗi frame, log mỗi 60 frame.
    pub stats: FrameStats,
}

impl RowLayoutCache {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            prev_grid_size: None,
            prev_display_offset: 0,
            prev_selection: None,
            prev_hovered_url: None,
            prev_ctrl_held: false,
            stats: FrameStats::default(),
        }
    }

    /// Đảm bảo `rows` có đúng `num_lines` entries — resize cache khi grid đổi.
    fn ensure_size(&mut self, num_lines: usize) {
        if self.rows.len() != num_lines {
            self.rows.clear();
            self.rows.reserve(num_lines);
            for _ in 0..num_lines {
                self.rows.push(RowLayout::empty());
            }
        }
    }
}

impl Default for RowLayoutCache {
    fn default() -> Self {
        Self::new()
    }
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
    /// Giống AtlasEngine `_p.rows` (ShapedRow cache).
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
            line_times,
            padding,
            cell_width_override,
            cursor_color_override,
            cursor_shape_override,
            row_cache,
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

    /// FNV-1a hash 1 display line — detect content change mà Term::damage()
    /// không track (input()/write_at_cursor() không gọi damage_line()).
    /// Hash bao gồm: char, fg, bg, flags, zerowidth, hyperlink.
    fn line_hash(cells: &[&IndexedCell]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x100_0000_01b3;
        let mut h = FNV_OFFSET;
        for ic in cells {
            let cell = &ic.cell;
            // char
            h ^= cell.c as u64;
            h = h.wrapping_mul(FNV_PRIME);
            // fg color (Named/Spec/Indexed → u64)
            h ^= Self::color_hash(cell.fg);
            h = h.wrapping_mul(FNV_PRIME);
            // bg color
            h ^= Self::color_hash(cell.bg);
            h = h.wrapping_mul(FNV_PRIME);
            // flags
            h ^= cell.flags.bits() as u64;
            h = h.wrapping_mul(FNV_PRIME);
            // zerowidth + hyperlink
            if let Some(zw) = cell.zerowidth() {
                for &c in zw {
                    h ^= c as u64;
                    h = h.wrapping_mul(FNV_PRIME);
                }
            }
            if let Some(hl) = cell.hyperlink() {
                for b in hl.uri().bytes() {
                    h ^= b as u64;
                    h = h.wrapping_mul(FNV_PRIME);
                }
            }
        }
        h
    }

    /// Convert alacritty Color → u64 cho hashing.
    fn color_hash(c: alacritty_terminal::vte::ansi::Color) -> u64 {
        use alacritty_terminal::vte::ansi::Color;
        match c {
            Color::Named(n) => n as u64,
            Color::Spec(rgb) => {
                0x1_0000 | (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
            }
            Color::Indexed(i) => 0x2_0000 | i as u64,
        }
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

    /// Kiểm tra char có thuộc box-drawing block (U+2500–U+257F) — các ký tự
    /// đường thẳng / khung mà Windows Terminal vẽ bằng primitive thay vì font.
    fn is_box_drawing(c: char) -> bool {
        matches!(c, '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}')
    }

    /// Tính geometry (pixel-perfect) cho box-drawing char trong cell.
    /// Trả list rect (x, y, w, h) tính bằng **device pixel** relative tới
    /// cell origin. Caller convert sang logical px khi paint.
    /// Giống AtlasEngine: light = 1 device px, heavy = 2, double = 2 line.
    fn box_drawing_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
        let cx = cw_d / 2;
        let cy = lh_d / 2;
        // Keep light/heavy thickness at 1/2 device px for crisp single lines.
        // Double-line strokes use a separate thickness so they can be tuned
        // independently (Windows Terminal AtlasEngine uses ~cellWidth/6).
        let t = 1; // light thickness
        let ht = 2; // heavy thickness
        // Double-line stroke width.  Using cellWidth/8 keeps two distinct strokes
        // even on smaller cells while staying closer to font glyph proportions.
        let dt = (cw_d as f32 / 8.0).round().max(1.0) as i32;
        let dl = dt; // offset from center to each double stroke
        let dv = dt;
        // Double-line stroke positions (device-pixel columns/rows).
        // `out` = closer to the cell edge that forms the corner's outer serif,
        // `in`  = closer to the cell center, forming the inner serif.
        let x_out = (cx - dl).max(0);
        let x_in = (cx + dl).min(cw_d - dt);
        let y_out = (cy - dv).max(0);
        let y_in = (cy + dv).min(lh_d - dt);
        // For horizontal strokes the pixel row is the top of the rect, so
        // we need the bottom row to sit on `y_out`/`y_in`.  Offset by dt
        // so the 1-px thick stroke occupies exactly that row.
        let y_out_top = (y_out - dt).max(0);
        let y_in_top = (y_in - dt).max(0);
        // Similarly, vertical strokes' left edge should sit on `x_out`/`x_in`.
        let x_out_left = (x_out - dt).max(0);
        let x_in_left = (x_in - dt).max(0);
        macro_rules! h {
            ($y:expr, $thick:expr) => {
                (0, $y, cw_d, $thick)
            };
        }
        macro_rules! v {
            ($x:expr, $thick:expr) => {
                ($x, 0, $thick, lh_d)
            };
        }
        // half horizontal: bắt đầu từ tâm cell
        macro_rules! hr {
            ($y:expr, $thick:expr) => {
                (cx, $y, cw_d - cx, $thick)
            };
        }
        macro_rules! hl {
            ($y:expr, $thick:expr) => {
                (0, $y, cx, $thick)
            };
        }
        macro_rules! vd {
            ($x:expr, $thick:expr) => {
                ($x, cy, $thick, lh_d - cy)
            };
        }
        macro_rules! vu {
            ($x:expr, $thick:expr) => {
                ($x, 0, $thick, cy)
            };
        }
        match c {
            '\u{2500}' => vec![h!(cy, t)],
            '\u{2501}' => vec![h!(cy, ht)],
            '\u{2502}' => vec![v!(cx, t)],
            '\u{2503}' => vec![v!(cx, ht)],
            '\u{250C}' => vec![vd!(cx, t), hr!(cy, t)],
            '\u{250D}' => vec![vd!(cx, ht), hr!(cy, t)],
            '\u{250E}' => vec![vd!(cx, t), hr!(cy, ht)],
            '\u{250F}' => vec![vd!(cx, ht), hr!(cy, ht)],
            '\u{2510}' => vec![vd!(cx, t), hl!(cy, t)],
            '\u{2511}' => vec![vd!(cx, ht), hl!(cy, t)],
            '\u{2512}' => vec![vd!(cx, t), hl!(cy, ht)],
            '\u{2513}' => vec![vd!(cx, ht), hl!(cy, ht)],
            '\u{2514}' => vec![vu!(cx, t), hr!(cy, t)],
            '\u{2515}' => vec![vu!(cx, ht), hr!(cy, t)],
            '\u{2516}' => vec![vu!(cx, t), hr!(cy, ht)],
            '\u{2517}' => vec![vu!(cx, ht), hr!(cy, ht)],
            '\u{2518}' => vec![vu!(cx, t), hl!(cy, t)],
            '\u{2519}' => vec![vu!(cx, ht), hl!(cy, t)],
            '\u{251A}' => vec![vu!(cx, t), hl!(cy, ht)],
            '\u{251B}' => vec![vu!(cx, ht), hl!(cy, ht)],
            '\u{251C}' => vec![v!(cx, t), hr!(cy, t)],
            '\u{251D}' => vec![v!(cx, ht), hr!(cy, t)],
            '\u{251E}' => vec![vu!(cx, ht), vd!(cx, t), hr!(cy, t)],
            '\u{251F}' => vec![vu!(cx, t), vd!(cx, ht), hr!(cy, t)],
            '\u{2520}' => vec![v!(cx, ht), hr!(cy, ht)],
            '\u{2521}' => vec![vu!(cx, ht), vd!(cx, t), hr!(cy, ht)],
            '\u{2522}' => vec![vu!(cx, t), vd!(cx, ht), hr!(cy, ht)],
            '\u{2523}' => vec![v!(cx, ht), hr!(cy, ht)],
            '\u{2524}' => vec![v!(cx, t), hl!(cy, t)],
            '\u{2525}' => vec![v!(cx, ht), hl!(cy, t)],
            '\u{2526}' => vec![vu!(cx, ht), vd!(cx, t), hl!(cy, t)],
            '\u{2527}' => vec![vu!(cx, t), vd!(cx, ht), hl!(cy, t)],
            '\u{2528}' => vec![v!(cx, ht), hl!(cy, ht)],
            '\u{2529}' => vec![vu!(cx, ht), vd!(cx, t), hl!(cy, ht)],
            '\u{252A}' => vec![vu!(cx, t), vd!(cx, ht), hl!(cy, ht)],
            '\u{252B}' => vec![v!(cx, ht), hl!(cy, ht)],
            '\u{252C}' => vec![h!(cy, t), vd!(cx, t)],
            '\u{252D}' => vec![hl!(cy, ht), hr!(cy, t), vd!(cx, t)],
            '\u{252E}' => vec![hl!(cy, t), hr!(cy, ht), vd!(cx, t)],
            '\u{252F}' => vec![h!(cy, ht), vd!(cx, t)],
            '\u{2530}' => vec![h!(cy, t), vd!(cx, ht)],
            '\u{2531}' => vec![hl!(cy, ht), hr!(cy, t), vd!(cx, ht)],
            '\u{2532}' => vec![hl!(cy, t), hr!(cy, ht), vd!(cx, ht)],
            '\u{2533}' => vec![h!(cy, ht), vd!(cx, ht)],
            '\u{2534}' => vec![h!(cy, t), vu!(cx, t)],
            '\u{2535}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, t)],
            '\u{2536}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, t)],
            '\u{2537}' => vec![h!(cy, ht), vu!(cx, t)],
            '\u{2538}' => vec![h!(cy, t), vu!(cx, ht)],
            '\u{2539}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, ht)],
            '\u{253A}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, ht)],
            '\u{253B}' => vec![h!(cy, ht), vu!(cx, ht)],
            '\u{253C}' => vec![h!(cy, t), v!(cx, t)],
            '\u{253D}' => vec![hl!(cy, ht), hr!(cy, t), v!(cx, t)],
            '\u{253E}' => vec![hl!(cy, t), hr!(cy, ht), v!(cx, t)],
            '\u{253F}' => vec![h!(cy, ht), v!(cx, t)],
            '\u{2540}' => vec![h!(cy, t), vu!(cx, ht), vd!(cx, t)],
            '\u{2541}' => vec![h!(cy, t), vu!(cx, t), vd!(cx, ht)],
            '\u{2542}' => vec![h!(cy, ht), v!(cx, ht)],
            '\u{2543}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, ht), vd!(cx, t)],
            '\u{2544}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, ht), vd!(cx, t)],
            '\u{2545}' => vec![hl!(cy, ht), hr!(cy, t), vu!(cx, t), vd!(cx, ht)],
            '\u{2546}' => vec![hl!(cy, t), hr!(cy, ht), vu!(cx, t), vd!(cx, ht)],
            '\u{2547}' => vec![h!(cy, ht), vu!(cx, ht), vd!(cx, t)],
            '\u{2548}' => vec![h!(cy, ht), vu!(cx, t), vd!(cx, ht)],
            '\u{2549}' => vec![hl!(cy, ht), hr!(cy, ht), vu!(cx, ht), vd!(cx, t)],
            '\u{254A}' => vec![hl!(cy, ht), hr!(cy, ht), vu!(cx, t), vd!(cx, ht)],
            '\u{254B}' => vec![h!(cy, ht), v!(cx, ht)],
            // dash — rải đoạn 2px on / 2px off
            '\u{2504}' | '\u{2506}' => Self::dash_h(cy, cw_d, t),
            '\u{2505}' | '\u{2507}' => Self::dash_h(cy, cw_d, ht),
            '\u{2508}' => Self::dash_v(cx, lh_d, t),
            '\u{2509}' => Self::dash_v(cx, lh_d, ht),
            // double lines
            '\u{2550}' => vec![(0, y_out_top, cw_d, dt), (0, y_in_top, cw_d, dt)],
            '\u{2551}' => vec![(x_out_left, 0, dt, lh_d), (x_in_left, 0, dt, lh_d)],
            // Corners: two nested empty rectangles.
            // ╔ double down-and-right
            '\u{2554}' => vec![
                (x_out_left, y_out_top, dt, lh_d - y_out_top),
                (x_out_left, y_out_top, cw_d - x_out_left, dt),
                (x_in_left, y_in_top, dt, lh_d - y_in_top),
                (x_in_left, y_in_top, cw_d - x_in_left, dt),
            ],
            // ╗ double down-and-left
            '\u{2557}' => vec![
                (x_in_left, y_out_top, dt, lh_d - y_out_top),
                (0, y_out_top, x_in_left + dt, dt),
                (x_out_left, y_in_top, dt, lh_d - y_in_top),
                (0, y_in_top, x_out_left + dt, dt),
            ],
            // ╚ double up-and-right
            '\u{255A}' => vec![
                (x_out_left, 0, dt, y_in),
                (x_out_left, y_in_top, cw_d - x_out_left, dt),
                (x_in_left, 0, dt, y_out),
                (x_in_left, y_out_top, cw_d - x_in_left, dt),
            ],
            // ╝ double up-and-left
            '\u{255D}' => vec![
                (x_in_left, 0, dt, y_in),
                (0, y_in_top, x_in_left + dt, dt),
                (x_out_left, 0, dt, y_out),
                (0, y_out_top, x_out_left + dt, dt),
            ],
            // Mixed-light double corners (best-effort, use center line for the light arm).
            // ╒ down single-and-right-double
            '\u{2552}' => vec![
                (cx, 0, t, lh_d),
                (cx, y_out, cw_d - cx, t),
                (cx, y_in, cw_d - cx, t),
            ],
            // ╓ down double-and-right-single
            '\u{2553}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (cx, y_out, cw_d - cx, t),
            ],
            // ╕ down single-and-left-double
            '\u{2555}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
            // ╖ down double-and-left-single
            '\u{2556}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
            // ╘ up single-and-right-double
            '\u{2558}' => vec![
                (cx, 0, t, lh_d),
                (cx, y_out, cw_d - cx, t),
                (cx, y_in, cw_d - cx, t),
            ],
            // ╙ up double-and-right-single
            '\u{2559}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (cx, y_out, cw_d - cx, t),
            ],
            // ╛ up single-and-left-double
            '\u{255B}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
            // ╜ up double-and-left-single
            '\u{255C}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, y_out, cx, t)],
            // Tee/cross pieces.
            // ╞ single vertical and right double
            '\u{255E}' => vec![
                (cx, 0, t, lh_d),
                (cx, y_out, cw_d - cx, t),
                (cx, y_in, cw_d - cx, t),
            ],
            // ╟ double vertical and right single
            '\u{255F}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (x_in, y_out, cw_d - x_in, t),
            ],
            // ╠ double vertical and right double
            '\u{2560}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (x_out + 1, y_out, cw_d - x_out - 1, t),
                (x_in + 1, y_in, cw_d - x_in - 1, t),
            ],
            // ╡ single vertical and left double
            '\u{2561}' => vec![(cx, 0, t, lh_d), (0, y_out, cx, t), (0, y_in, cx, t)],
            // ╢ double vertical and left single
            '\u{2562}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (0, y_out, x_out + 1, t),
            ],
            // ╣ double vertical and left double
            '\u{2563}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (0, y_out, x_out + 1, t),
                (0, y_in, x_in + 1, t),
            ],
            // ╤ down single and horizontal double
            '\u{2564}' => vec![
                (cx, y_out, t, lh_d - y_out),
                (0, y_out, cw_d, t),
                (0, y_in, cw_d, t),
            ],
            // ╥ down double and horizontal single
            '\u{2565}' => vec![
                (x_out, y_out, t, lh_d - y_out),
                (x_in, y_out, t, lh_d - y_out),
                (0, cy, cw_d, t),
            ],
            // ╦ down double and horizontal double
            '\u{2566}' => vec![
                (x_out, y_out, t, lh_d - y_out),
                (x_in, y_out, t, lh_d - y_out),
                (x_out, y_in, t, lh_d - y_in),
                (x_in, y_in, t, lh_d - y_in),
                (0, y_out, cw_d, t),
                (0, y_in, cw_d, t),
            ],
            // ╧ up single and horizontal double
            '\u{2567}' => vec![
                (cx, 0, t, y_out + 1),
                (0, y_out, cw_d, t),
                (0, y_in, cw_d, t),
            ],
            // ╨ up double and horizontal single
            '\u{2568}' => vec![
                (x_out, 0, t, y_out + 1),
                (x_in, 0, t, y_out + 1),
                (0, cy, cw_d, t),
            ],
            // ╩ up double and horizontal double
            '\u{2569}' => vec![
                (x_out, 0, t, y_out + 1),
                (x_in, 0, t, y_out + 1),
                (x_out, y_in, t, lh_d - y_in),
                (x_in, y_in, t, lh_d - y_in),
                (0, y_out, cw_d, t),
                (0, y_in, cw_d, t),
            ],
            // Crosses.
            // ╪ vertical single and horizontal double
            '\u{256A}' => vec![(cx, 0, t, lh_d), (0, y_out, cw_d, t), (0, y_in, cw_d, t)],
            // ╫ vertical double and horizontal single
            '\u{256B}' => vec![(x_out, 0, t, lh_d), (x_in, 0, t, lh_d), (0, cy, cw_d, t)],
            // ╬ double vertical and horizontal double
            '\u{256C}' => vec![
                (x_out, 0, t, lh_d),
                (x_in, 0, t, lh_d),
                (0, y_out, cw_d, t),
                (0, y_in, cw_d, t),
            ],
            '\u{256D}' => vec![vd!(cx, t), hr!(cy, t)],
            '\u{256E}' => vec![vd!(cx, t), hl!(cy, t)],
            '\u{256F}' => vec![vu!(cx, t), hl!(cy, t)],
            '\u{2570}' => vec![vu!(cx, t), hr!(cy, t)],
            '\u{2574}' => vec![hl!(cy, t)],
            '\u{2575}' => vec![vu!(cx, t)],
            '\u{2576}' => vec![hr!(cy, t)],
            '\u{2577}' => vec![vd!(cx, t)],
            '\u{2578}' => vec![hl!(cy, ht)],
            '\u{2579}' => vec![vu!(cx, ht)],
            '\u{257A}' => vec![hr!(cy, ht)],
            '\u{257B}' => vec![vd!(cx, ht)],
            // ── Block elements (U+2580–U+259F) ──
            // pi dùng ▀▄ cho input box padding, ▌ cho diff marker, █ cho fill.
            // Vẽ primitive → pixel-perfect, không font AA blur.
            '\u{2580}' => vec![(0, 0, cw_d, cy)], // ▀ upper half
            '\u{2581}' => vec![(0, lh_d - lh_d / 8, cw_d, lh_d / 8)], // ▁ lower 1/8
            '\u{2582}' => vec![(0, lh_d - lh_d / 4, cw_d, lh_d / 4)], // ▂ lower 1/4
            '\u{2583}' => vec![(0, lh_d - 3 * lh_d / 8, cw_d, 3 * lh_d / 8)], // ▃
            '\u{2584}' => vec![(0, cy, cw_d, lh_d - cy)], // ▄ lower half
            '\u{2585}' => vec![(0, lh_d - 5 * lh_d / 8, cw_d, 5 * lh_d / 8)], // ▅
            '\u{2586}' => vec![(0, lh_d - lh_d / 4, cw_d, lh_d / 4 * 3)], // ▆ lower 3/4
            '\u{2587}' => vec![(0, lh_d / 8, cw_d, lh_d - lh_d / 8)], // ▇ lower 7/8
            '\u{2588}' => vec![(0, 0, cw_d, lh_d)], // █ full block
            '\u{2589}' => vec![(0, 0, 7 * cw_d / 8, lh_d)], // ▉ left 7/8
            '\u{258A}' => vec![(0, 0, 3 * cw_d / 4, lh_d)], // ▊ left 3/4
            '\u{258B}' => vec![(0, 0, 5 * cw_d / 8, lh_d)], // ▋ left 5/8
            '\u{258C}' => vec![(0, 0, cx, lh_d)], // ▌ left half
            '\u{258D}' => vec![(0, 0, 3 * cw_d / 8, lh_d)], // ▍ left 3/8
            '\u{258E}' => vec![(0, 0, cw_d / 4, lh_d)], // ▎ left 1/4
            '\u{258F}' => vec![(0, 0, cw_d / 8, lh_d)], // ▏ left 1/8
            '\u{2594}' => vec![(0, 0, cw_d, lh_d / 8)], // ▔ upper 1/8
            '\u{2595}' => vec![(cw_d - cw_d / 8, 0, cw_d / 8, lh_d)], // ▕ right 1/8
            // Quadrant blocks
            '\u{2596}' => vec![(0, cy, cx, lh_d - cy)], // ▖ quad lower-left
            '\u{2597}' => vec![(cx, cy, cw_d - cx, lh_d - cy)], // ▗ quad lower-right
            '\u{2598}' => vec![(0, 0, cx, cy)],         // ▘ quad upper-left
            '\u{2599}' => vec![
                (0, 0, cx, cy),
                (0, cy, cx, lh_d - cy),
                (cx, 0, cw_d - cx, cy),
            ], // ▙
            '\u{259A}' => vec![(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)], // ▚
            '\u{259B}' => vec![(0, 0, cw_d, cy), (0, cy, cx, lh_d - cy)], // ▛
            '\u{259C}' => vec![(0, 0, cw_d, cy), (cx, cy, cw_d - cx, lh_d - cy)], // ▜
            '\u{259D}' => vec![(cx, 0, cw_d - cx, cy)], // ▝ quad upper-right
            '\u{259E}' => vec![(cx, 0, cw_d - cx, cy), (0, cy, cx, lh_d - cy)], // ▞
            '\u{259F}' => vec![
                (0, 0, cx, cy),
                (cx, 0, cw_d - cx, cy),
                (cx, cy, cw_d - cx, lh_d - cy),
            ], // ▟
            // ── Right half block (U+2590) — mirror của ▌ left half ──
            '\u{2590}' => vec![(cw_d - cx, 0, cx, lh_d)], // ▐ right half
            // ── Shade blocks (U+2591–U+2593) — stipple bằng device pixel grid ──
            c @ ('\u{2591}' | '\u{2592}' | '\u{2593}') => Self::shade_rects(c, cw_d, lh_d),
            // diagonal / quadruple-dash → fallback font (hiếm trong TUI)
            _ => vec![],
        }
    }

    /// Shade blocks (U+2591 light, U+2592 medium, U+2593 dark).
    /// Vẽ stipple pattern bang 1x1 device pixel dots.
    fn shade_rects(c: char, cw_d: i32, lh_d: i32) -> Vec<(i32, i32, i32, i32)> {
        if cw_d * lh_d > 1024 {
            return vec![];
        }
        let mut out = Vec::new();
        match c {
            '\u{2591}' => {
                for y in 0..lh_d {
                    for x in 0..cw_d {
                        if (x + y) % 2 == 0 {
                            out.push((x, y, 1, 1));
                        }
                    }
                }
            }
            '\u{2592}' => {
                for y in 0..lh_d {
                    for x in 0..cw_d {
                        if x % 2 == 0 {
                            out.push((x, y, 1, 1));
                        }
                    }
                }
            }
            '\u{2593}' => {
                for y in 0..lh_d {
                    for x in 0..cw_d {
                        if (x + y) % 2 != 0 {
                            out.push((x, y, 1, 1));
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    fn dash_h(y: i32, w: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
        let mut out = Vec::new();
        let mut x = 0;
        while x < w {
            let ew = 2.min(w - x);
            out.push((x, y, ew, thick));
            x += 4;
        }
        out
    }

    fn dash_v(x: i32, h: i32, thick: i32) -> Vec<(i32, i32, i32, i32)> {
        let mut out = Vec::new();
        let mut y = 0;
        while y < h {
            let eh = 2.min(h - y);
            out.push((x, y, thick, eh));
            y += 4;
        }
        out
    }

    /// Layout 1 display row — build rects + text runs + box draws cho cells
    /// trên cùng 1 dòng. Trích từ `layout_grid` cũ, tách per-row để cache.
    ///
    /// `line_cells` — cells thuộc 1 display line (cùng `point.line`).
    /// `display_line` — index 0-based từ top viewport.
    fn layout_row(
        line_cells: Vec<&IndexedCell>,
        display_line: i32,
        theme: &TerminalTheme,
        base_font: &Font,
        selection_set: &HashSet<LayoutPoint>,
        hovered_url: Option<&super::url::DetectedUrl>,
        ctrl_held: bool,
    ) -> RowLayout {
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
            // Skip space theo emoji variation sequence.
            if cell.c == ' ' && prev_had_extras {
                prev_had_extras = false;
                continue;
            }
            prev_had_extras = matches!(cell.zerowidth(), Some(c) if !c.is_empty());

            let lp = LayoutPoint {
                line: display_line,
                column: point.column.0 as i32,
            };
            let _is_selected = selection_set.contains(&lp);

            // ── AtlasEngine color bitmap caching ──
            // Skip cell_colors() cho blank cells (space + default bg +
            // no flags) — không cần fg hay bg → skip resolve_cell_color()
            // + ensure_minimum_contrast() + DIM alpha.
            // Tiết kiệm ~80% color resolution cho dòng prompt trống.
            if Self::is_blank(cell) {
                continue;
            }

            let (fg, bg) = Self::cell_colors(cell, theme);

            // Nền khác default → rect.
            // AtlasEngine: merge adjacent same-color cells thành 1 quad
            // để giảm paint_quad calls (coalescing).
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

            let mut style = Self::cell_style(cell, fg, base_font);
            // Ctrl+hover URL highlight.
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

            // Box-drawing chars — vẽ primitive thay vì font glyph.
            // AtlasEngine: text run không bị interrupt bởi box-drawing —
            // insert space placeholder vào batch để giữ position
            // continuity, giảm số runs → giảm shape_line calls.
            // Box-drawing primitive vẽ trên cùng, che space invisible.
            //
            // ⚠️ GIỮ nguyên underline/strikethrough của space placeholder —
            // không strip. Lý do:
            //   1. Box-drawing primitive vẽ line ở cell CENTER (cy/cx),
            //      text underline vẽ ở BASELINE — khác vị trí, không duplicate.
            //   2. Nếu strip underline → can_append() fail (underline mismatch)
            //      → batch bị SPLIT → text underline đứt đoạn tại mỗi
            //      box-drawing position → "đường kẻ không liền mạch".
            //   3. Giữ underline → text run liền mạch → underline liên tục.
            //      Box-drawing primitive ở vị trí khác nên không xung đột.
            if Self::is_box_drawing(cell.c) && !Self::box_drawing_rects(cell.c, 16, 16).is_empty() {
                box_draws.push(BoxDrawCell {
                    point: lp,
                    color: style.color,
                    c: cell.c,
                });
                // Insert space vào current batch — giữ nguyên style (incl.
                // underline/strikethrough) để can_append() thành công,
                // text run không bị split, underline liên tục.
                let mut sp = style;
                sp.len = ' '.len_utf8();
                if let Some(b) = current_batch.as_mut() {
                    if b.start.column + b.cell_count as i32 == lp.column && b.can_append(&sp) {
                        b.append_char(' ');
                    } else {
                        // Column gap hoặc style incompatible — flush và tạo batch mới.
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

    /// Update row cache: chỉ recompute layout cho dirty rows, reuse cached
    /// artifacts cho non-dirty rows. Giống AtlasEngine `_p.invalidatedRows`.
    ///
    /// Dirty sources:
    /// - `TermDamageInfo::Full` → invalidate all.
    /// - `TermDamageInfo::Partial(lines)` → invalidate those display lines.
    /// - Grid size change / selection / hover / ctrl change → invalidate all
    ///   (global state affect per-cell styling).
    /// - **Scroll-only change** (`display_offset` đổi, không có global change
    ///   khác) → **shift cache rows** thay vì full invalidate. Chỉ recompute
    ///   rows mới visible (top/bottom `|delta|` rows). Giống AtlasEngine
    ///   `_p.rows` shift khi scroll.
    #[allow(clippy::too_many_arguments)]
    fn update_row_cache(
        cache: &mut RowLayoutCache,
        cells: &[IndexedCell],
        damage: &TermDamageInfo,
        num_lines: usize,
        display_offset: usize,
        grid_size: (u16, u16),
        selection: Option<alacritty_terminal::selection::SelectionRange>,
        hovered_url: Option<&super::url::DetectedUrl>,
        ctrl_held: bool,
        theme: &TerminalTheme,
        base_font: &Font,
        selection_set: &HashSet<LayoutPoint>,
        cursor_display_line: i32,
    ) {
        use itertools::Itertools;

        // ── Detect global state changes → full invalidate ──
        let size_changed = cache.prev_grid_size != Some(grid_size);
        let scroll_delta = display_offset as i32 - cache.prev_display_offset as i32;
        let scroll_changed = scroll_delta != 0;
        let selection_changed = cache.prev_selection != selection;
        let hover_changed = cache.prev_hovered_url.as_ref() != hovered_url;
        let ctrl_changed = cache.prev_ctrl_held != ctrl_held;
        // Scroll-only: chỉ display_offset đổi, không có size/selection/hover/ctrl
        // → shift cache rows thay vì full invalidate.
        let scroll_only = scroll_changed
            && !size_changed
            && !selection_changed
            && !hover_changed
            && !ctrl_changed;
        let global_invalidate = size_changed
            || (scroll_changed && !scroll_only)
            || selection_changed
            || hover_changed
            || ctrl_changed;

        // Ensure cache has correct number of rows.
        cache.ensure_size(num_lines);

        // ── Scroll shift: di chuyển cached rows thay vì recompute tất cả ──
        // Giống AtlasEngine `_p.rows` shift khi scroll.
        //
        // scroll_delta > 0 (scroll UP): rotate_right(delta)
        //   → rows[delta..] = old rows[0..num_lines-delta] (shifted down)
        //   → top `delta` rows stale → cần recompute (mới visible)
        // scroll_delta < 0 (scroll DOWN): rotate_left(|delta|)
        //   → rows[0..num_lines-|delta|] = old rows[|delta|..] (shifted up)
        //   → bottom |delta| rows stale → cần recompute (mới visible)
        let mut scroll_dirty: Vec<usize> = Vec::new();
        if scroll_only {
            if scroll_delta > 0 {
                let delta = (scroll_delta as usize).min(num_lines);
                if delta < num_lines {
                    cache.rows.rotate_right(delta);
                    scroll_dirty = (0..delta).collect();
                } else {
                    scroll_dirty = (0..num_lines).collect();
                }
            } else if scroll_delta < 0 {
                let delta = ((-scroll_delta) as usize).min(num_lines);
                if delta < num_lines {
                    cache.rows.rotate_left(delta);
                    scroll_dirty = ((num_lines - delta)..num_lines).collect();
                } else {
                    scroll_dirty = (0..num_lines).collect();
                }
            }
        }

        // Determine which rows are dirty.
        let full_dirty = global_invalidate || matches!(damage, TermDamageInfo::Full);
        let dirty_set: HashSet<usize> = if full_dirty {
            (0..num_lines).collect()
        } else if scroll_only {
            // Scroll shift: dirty = scroll_dirty + Term::damage() partial
            let mut ds: HashSet<usize> = scroll_dirty.into_iter().collect();
            if let TermDamageInfo::Partial(lines) = damage {
                for &l in lines.iter() {
                    if l < num_lines {
                        ds.insert(l);
                    }
                }
            }
            ds
        } else if let TermDamageInfo::Partial(lines) = damage {
            lines.iter().copied().filter(|&l| l < num_lines).collect()
        } else {
            HashSet::new()
        };

        // Update stats.
        cache.stats.total_lines = num_lines;
        cache.stats.dirty_lines = dirty_set.len();
        cache.stats.hash_calls = 0;
        // ── Group cells by display line ──
        // cells trong snapshot là display order (top → bottom),
        // group bằng grid line (point.line).
        let linegroups = cells.iter().chunk_by(|ic| ic.point.line);
        for (display_line, (_, line_cells)) in linegroups.into_iter().enumerate() {
            if display_line >= num_lines {
                break;
            }
            // Collect cells for this line (need Vec để iterate được).
            let line_vec: Vec<&IndexedCell> = line_cells.collect();

            // ── Dirty detection: damage + content hash ──
            // Term::damage() không track input()/write_at_cursor(),
            // nhưng những thay đổi này chỉ xảy ra tại dòng cursor.
            // Chỉ hash cursor display line thay vì hash tất cả non-dirty
            // rows mỗi frame → giảm từ ~24 hash ops/frame xuống ≤1.
            let is_dirty = if dirty_set.contains(&display_line) {
                true
            } else if display_line as i32 == cursor_display_line
                && cursor_display_line >= 0
                && cursor_display_line < num_lines as i32
            {
                // Hash cursor line — fallback cho input()/write_at_cursor().
                cache.stats.hash_calls += 1;
                let hashed = Self::line_hash(&line_vec);
                hashed != cache.rows[display_line].prev_hash
            } else {
                // Non-dirty, non-cursor row → trust cache, skip hash.
                false
            };

            if is_dirty {
                // Compute hash mới + recompute layout.
                let new_hash = Self::line_hash(&line_vec);
                let layout = Self::layout_row(
                    line_vec,
                    display_line as i32,
                    theme,
                    base_font,
                    selection_set,
                    hovered_url,
                    ctrl_held,
                );
                cache.rows[display_line] = RowLayout {
                    rects: layout.rects,
                    runs: layout.runs,
                    box_draws: layout.box_draws,
                    shaped_lines: Vec::new(), // sẽ fill ở prepaint
                    prev_hash: new_hash,
                };
            }
            // Non-dirty row → giữ cached RowLayout (incl. shaped_lines) nguyên.
        }

        // ── Update prev state ──
        cache.prev_grid_size = Some(grid_size);
        cache.prev_display_offset = display_offset;
        cache.prev_selection = selection;
        cache.prev_hovered_url = hovered_url.cloned();
        cache.prev_ctrl_held = ctrl_held;
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

    /// Paint text run dùng cached `ShapedLine` (đã shape ở prepaint).
    /// Không gọi `shape_line` ở đây — skip hoàn toàn cho non-dirty rows.
    /// Giống AtlasEngine `ShapedRow` — glyph data persisted, paint chỉ read.
    ///
    /// `x`, `y` đã là device-pixel snapped logical coords — không re-snap.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        shaped: &ShapedLine,
        x: Pixels,
        y: Pixels,
        _cell_w: Pixels,
        line_h: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let pos = point(x, y);
        let _ = shaped.paint(pos, line_h, TextAlign::Left, None, window, cx);
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
        // Terminal rendering là grid-based; nếu cell metrics là float
        // logical px, các dòng/cột nằm ở tọa độ subpixel → glyph rasterize
        // bị anti-alias không nhất quán → đường kẻ/box-drawing nhòe.
        // Snap line_height + cell_width sang device pixel nguyên (giống
        // Windows Terminal AtlasEngine + Zed terminal_element).
        let scale_factor = window.scale_factor().max(1.0);
        let snap_px = |value: f32| -> f32 { (value * scale_factor).round() / scale_factor };

        // Font measure.
        let font_id = cx.text_system().resolve_font(&self.font);
        let font_px = self.font_size;
        let cell_width = if let Some(cw) = self.cell_width_override {
            px(snap_px(cw))
        } else {
            // Windows Terminal / CSS ch unit: đo advance width của '0'
            // thay vì 'm' — matches monospace cell width chính xác hơn.
            let raw = cx
                .text_system()
                .ch_advance(font_id, font_px)
                .map(|s| f32::from(s))
                .unwrap_or_else(|_| {
                    // Fallback: đo 'm' advance nếu '0' không có glyph.
                    cx.text_system()
                        .advance(font_id, font_px, 'm')
                        .map(|s| f32::from(s.width))
                        .unwrap_or(8.0)
                });
            px(snap_px(raw))
        };
        // Line height — đảm bảo tối thiểu = ascent + descent (font metrics)
        // để text không bị clip, giống Windows Terminal dùng DWRITE_FONT_METRICS.
        // GPUI paint_line tự center text trong line_height dựa trên layout
        // ascent/descent, nên chỉ cần đảm bảo line_height đủ lớn.
        let font_ascent = cx.text_system().ascent(font_id, font_px);
        let font_descent = cx.text_system().descent(font_id, font_px);
        let natural_line_height = f32::from(font_ascent) + f32::from(font_descent);
        let factor_height = f32::from(font_px) * self.line_height_factor;
        // max(factor_height, natural_line_height) → không bao giờ clip.
        let line_height = px(snap_px(factor_height.max(natural_line_height)));

        // ── Padding (config) ──
        let pad_left = px(self.padding.left);
        let pad_right = px(self.padding.right);
        let pad_top = px(self.padding.top);
        let pad_bottom = px(self.padding.bottom);

        // ── Gutter: [HH:MM:SS] line_number ──
        // Chiều rộng gutter = chiều rộng template text + padding.
        // Template width auto-expand theo số digit của total_lines (min 2 digit).
        // line_times.len() == total_lines (view duy trì synced).
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
        let gutter_width = gutter_text_width + px(8.0); // 4px padding mỗi bên gutter

        // Resize session theo bounds (race-free: chỉ khi đổi).
        // Trừ gutter_width + pad_left + pad_right khỏi chiều rộng.
        // Tính rows/cols bằng device pixels (snap) để grid khít pixel grid.
        let grid_width = (f32::from(bounds.size.width)
            - f32::from(gutter_width)
            - f32::from(pad_left)
            - f32::from(pad_right))
        .max(f32::from(cell_width));
        let grid_width_device = (grid_width * scale_factor).floor().max(1.0);
        let cell_width_device = f32::from(cell_width) * scale_factor;
        let cols = ((grid_width_device / cell_width_device).floor() as u16).max(1);
        // Trừ pad_top + pad_bottom khỏi chiều cao.
        let avail_height =
            f32::from(bounds.size.height) - f32::from(pad_top) - f32::from(pad_bottom);
        let avail_height_device = (avail_height * scale_factor).floor().max(0.0);
        let line_height_device = f32::from(line_height) * scale_factor;
        let rows = ((avail_height_device / line_height_device).floor() as u16).max(1);
        if self.last_size != Some((rows, cols)) {
            self.session.update(cx, |s, _| s.resize(rows, cols));
            self.last_size = Some((rows, cols));
        }

        // Snapshot tươi (sau resize grid).
        let snapshot = self.session.read(cx).snapshot();
        let num_lines = snapshot.terminal_bounds.num_lines;
        let num_cols = snapshot.terminal_bounds.num_cols;
        let display_offset = snapshot.display_offset;
        let total_lines = snapshot.total_lines;

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

        // Cursor display line — dùng cho content hash fallback.
        // Term::damage() không track input()/write_at_cursor() — những thay
        // đổi này chỉ xảy ra tại dòng cursor. Chỉ hash dòng đó thay vì
        // hash tất cả non-dirty rows mỗi frame.
        let cursor_display_line = snapshot.cursor.point.line.0 + display_offset as i32;

        // ── Update row cache: chỉ recompute dirty rows ──
        // Giống AtlasEngine `_p.invalidatedRows` — skip layout cho non-dirty rows.
        // Cache persists qua frames trong Rc<RefCell<>>.
        Self::update_row_cache(
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

        // ── Fill cached ShapedLine cho runs chưa được shape ──
        // Giống AtlasEngine `ShapedRow` — glyph data persisted qua frames.
        // Non-dirty row: shaped_lines đã có từ frame trước → skip hoàn toàn.
        // Dirty row: shaped_lines = empty → shape_line + cache.
        // Chuyển shape_line cost từ paint → prepaint (paint chỉ read).
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
        // Override shape từ config (Block/Bar/Underline) — giống Windows Terminal
        // tôn trọng user setting. Shell có thể set Hidden để ẩn cursor.
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
                    // Map config shape → alacritty CursorShape.
                    let shape = match self.cursor_shape_override {
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
            }
        };

        let _hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);

        // ── Gutter entries: [HH:MM:SS] line_number cho mỗi dòng hiển thị ──
        // Timestamp per-line: lấy từ line_times (tracked khi output mới).
        // Gutter background — từ theme (có thể bị override bởi config colors.gutter_bg).
        let gutter_bg = self.theme.gutter_bg;
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
                        clock_len: 0,
                        y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
                    };
                }
                // Line number 1-based, absolute trong scrollback.
                // abs_line (0-based) = total_lines - display_offset - num_lines + i
                let line_num =
                    total_lines as i32 - display_offset as i32 - num_lines as i32 + i as i32 + 1;
                let line_num = line_num.max(1) as usize;
                // 0-based index into line_times (absolute grid position, NOT adjusted by offset).
                let abs_idx = (total_lines as i32 - display_offset as i32 - num_lines as i32
                    + i as i32)
                    .max(0) as usize;
                let time_str = if abs_idx < lt.len() {
                    lt[abs_idx].as_str()
                } else {
                    "--:--:--"
                };
                let text = format!("[{}] {:>width$}", time_str, line_num, width = num_digits);
                // Byte length của phần clock "[HH:MM:SS] " = 1 + time_str + 2 ("[" + "] ").
                let clock_len = 1 + time_str.len() + 2;
                GutterEntry {
                    text: SharedString::from(text),
                    clock_len,
                    y: px(snap_px(f32::from(bounds.origin.y + i as f32 * line_height))),
                }
            })
            .collect();

        // Grid origin = bên phải gutter + pad_left, pad_top.
        // Snap origin sang device pixel grid để tránh subpixel jitter.
        let grid_origin = GpuiPoint {
            x: px(snap_px(f32::from(
                bounds.origin.x + gutter_width + pad_left,
            ))),
            y: px(snap_px(f32::from(bounds.origin.y + pad_top))),
        };

        // Sink metrics cho View (mouse/wheel).
        // gutter_width trong metrics bao gồm pad_left để pixel_to_grid
        // convert chính xác từ tọa độ mouse.
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
        // Đăng ký IME input handler (chỉ active khi focus).
        window.handle_input(
            &self.focus,
            gpui::ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            // ── Frame stats counters (Bước 3: measurement) ──
            let mut quad_count: usize = 0;
            let mut run_count: usize = 0;

            // Nền terminal.
            window.paint_quad(fill(bounds, layout.background));
            quad_count += 1;

            // ── Gutter: [HH:MM:SS] line_number ──
            let gw = layout.gutter_width;
            if gw > px(0.0) {
                // Nền gutter.
                let gutter_bounds = Bounds {
                    origin: bounds.origin,
                    size: size(gw, bounds.size.height),
                };
                window.paint_quad(fill(gutter_bounds, layout.gutter_bg));
                quad_count += 1;
                // Gutter text cho mỗi dòng — 2 TextRuns: clock + line number.
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
                            // Empty entry hoặc fallback — single run.
                            vec![TextRun {
                                len: entry.text.len(),
                                color: clock_color,
                                background_color: None,
                                font: self.font.clone(),
                                underline: None,
                                strikethrough: None,
                            }]
                        };
                    let line =
                        window
                            .text_system()
                            .shape_line(entry.text.clone(), gfont_px, &runs, None);
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

            // Device-pixel grid coordinates — đảm bảo các cell liền kề khít
            // nhau chính xác, không gap/overlap do round/floor mismatch.
            // Mỗi cell có origin + col*cw_d / row*lh_d là device-pixel integer,
            // convert ngược sang logical khi gọi paint_quad.
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

            // ── Per-row paint: đọc từ RowLayoutCache ──
            // Giống AtlasEngine `_p.rows` — iterate cached ShapedRow[],
            // paint rects + runs + box_draws cho mỗi row.
            let num_lines = layout.num_lines;
            let cache = self.row_cache.borrow();

            // Cell bg rects — per row.
            // Dùng loop index `i` cho Y position (không dùng `r.point.line`)
            // → cache position-independent, hỗ trợ scroll shift.
            // AtlasEngine: adjacent same-color cells đã được coalesce thành 1 rect
            // trong layout_row → 1 paint_quad per contiguous run thay vì 1 per cell.
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

            // Selection highlight (sau bg rects, trước text để text hiện trên nền).
            for r in &layout.selection_rects {
                let pos = point(cell_x(r.point.column), cell_y(r.point.line));
                let sz = size(run_w(r.num_cells), line_h_px);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
                quad_count += 1;
            }

            // Text runs — per row.
            // Dùng cached ShapedLine (đã shape ở prepaint) — skip shape_line
            // hoàn toàn cho non-dirty rows. Giống AtlasEngine `ShapedRow` paint.
            // Dùng loop index `i` cho Y position → cache position-independent.
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

            // Box-drawing primitive — per row.
            // Dùng loop index `i` cho Y position → cache position-independent.
            for i in 0..num_lines {
                let cell_y_logical = cell_y(i as i32);
                for bd in &cache.rows[i].box_draws {
                    let cell_x_logical = cell_x(bd.point.column);
                    for (rx, ry, rw, rh) in Self::box_drawing_rects(bd.c, cw_d, lh_d) {
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
            drop(cache); // Release borrow trước khi update stats.

            // Cursor — vẽ theo shape (Block/Bar/Underline), có blink.
            if let Some(cur) = &layout.cursor {
                // Quyết định có vẽ cursor không:
                // - Không focus → luôn vẽ (để user thấy cursor ở đâu).
                // - Focus + blink on → chỉ vẽ khi cursor_visible.
                // - Focus + blink off → luôn vẽ.
                let should_paint = !self.focused || self.cursor_visible;
                if should_paint {
                    let pos = point(cell_x(cur.point.column), cell_y(cur.point.line));
                    let sz = match cur.shape {
                        CursorShape::Beam => {
                            // Thanh dọc: 20% cell width, full height.
                            // Snap width lên device pixel để tránh subpixel blur.
                            let bar_w = (cw * 0.2).max(px(1.0));
                            let bar_w_d = (f32::from(bar_w) * scale_factor).ceil().max(1.0) as i32;
                            size(px(bar_w_d as f32 / scale_factor), line_h_px)
                        }
                        CursorShape::Underline => {
                            // Gạch dưới: full width, 15% line height (min 2px).
                            let ul_h = (lh * 0.15).max(px(2.0));
                            let ul_h_d = (f32::from(ul_h) * scale_factor).ceil().max(2.0) as i32;
                            size(run_w(1), px(ul_h_d as f32 / scale_factor))
                        }
                        CursorShape::Block => {
                            // Block đầy: full cell — snap width lên device pixel
                            // để khít grid, không subpixel gap (giống Windows Terminal).
                            size(run_w(1), line_h_px)
                        }
                        CursorShape::HollowBlock => {
                            // Hollow block: vẽ border (không fill) — fallback
                            // về block đầy cho đơn giản.
                            size(run_w(1), line_h_px)
                        }
                        CursorShape::Hidden => return,
                    };
                    window.paint_quad(fill(Bounds::new(pos, sz), cur.color));
                    quad_count += 1;
                }
            }

            // ── Update frame stats + log mỗi 60 frame (~1s) ──
            // Bước 3: measurement — đếm paint calls để đo bottleneck.
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
