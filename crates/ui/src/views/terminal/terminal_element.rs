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
    GlobalElementId, Hsla, IntoElement, LayoutId, Pixels, Point as GpuiPoint, SharedString,
    TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, relative, size,
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

/// Một dòng gutter: text + vị trí pixel (top-left) + byte length của phần clock.
struct GutterEntry {
    text: SharedString,
    /// Byte length của phần clock "[HH:MM:SS] " (không bao gồm line number).
    clock_len: usize,
    y: Pixels,
}

/// Thông tin layout computed ở prepaint → paint.
pub struct LayoutState {
    rects: Vec<LayoutRect>,
    /// Selection highlight rects (painted between bg rects and text).
    selection_rects: Vec<LayoutRect>,
    runs: Vec<BatchedTextRun>,
    /// Box-drawing cells — vẽ primitive thay vì font glyph.
    box_draws: Vec<BoxDrawCell>,
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
    /// Per-line timestamps for gutter (0 = oldest line).
    line_times: Vec<String>,
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
        let t = 1; // light thickness (device px)
        let ht = 2; // heavy thickness
        let dl = (cw_d / 6).max(1); // double-line horizontal offset
        let dv = (lh_d / 6).max(1); // double-line vertical offset
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
            '\u{2550}' => vec![h!(cy - dv, t), h!(cy + dv, t)],
            '\u{2551}' => vec![v!(cx - dl, t), v!(cx + dl, t)],
            '\u{2552}' => vec![vd!(cx - dl, t), hr!(cy, t)],
            '\u{2553}' => vec![vd!(cx, t), hr!(cy - dv, t), hr!(cy + dv, t)],
            '\u{2554}' => vec![
                vd!(cx - dl, t),
                vd!(cx + dl, t),
                hr!(cy - dv, t),
                hr!(cy + dv, t),
            ],
            '\u{2555}' => vec![vd!(cx + dl, t), hl!(cy, t)],
            '\u{2556}' => vec![vd!(cx, t), hl!(cy - dv, t), hl!(cy + dv, t)],
            '\u{2557}' => vec![
                vd!(cx - dl, t),
                vd!(cx + dl, t),
                hl!(cy - dv, t),
                hl!(cy + dv, t),
            ],
            '\u{2558}' => vec![vu!(cx - dl, t), hr!(cy, t)],
            '\u{2559}' => vec![vu!(cx, t), hr!(cy - dv, t), hr!(cy + dv, t)],
            '\u{255A}' => vec![
                vu!(cx - dl, t),
                vu!(cx + dl, t),
                hr!(cy - dv, t),
                hr!(cy + dv, t),
            ],
            '\u{255B}' => vec![vu!(cx + dl, t), hl!(cy, t)],
            '\u{255C}' => vec![vu!(cx, t), hl!(cy - dv, t), hl!(cy + dv, t)],
            '\u{255D}' => vec![
                vu!(cx - dl, t),
                vu!(cx + dl, t),
                hl!(cy - dv, t),
                hl!(cy + dv, t),
            ],
            '\u{255E}' => vec![v!(cx - dl, t), v!(cx + dl, t), hr!(cy, t)],
            '\u{255F}' => vec![v!(cx, t), hr!(cy - dv, t), hr!(cy + dv, t)],
            '\u{2560}' => vec![
                v!(cx - dl, t),
                v!(cx + dl, t),
                hr!(cy - dv, t),
                hr!(cy + dv, t),
            ],
            '\u{2561}' => vec![v!(cx - dl, t), v!(cx + dl, t), hl!(cy, t)],
            '\u{2562}' => vec![v!(cx, t), hl!(cy - dv, t), hl!(cy + dv, t)],
            '\u{2563}' => vec![
                v!(cx - dl, t),
                v!(cx + dl, t),
                hl!(cy - dv, t),
                hl!(cy + dv, t),
            ],
            '\u{2564}' => vec![h!(cy - dv, t), h!(cy + dv, t), vd!(cx, t)],
            '\u{2565}' => vec![h!(cy, t), vd!(cx - dl, t), vd!(cx + dl, t)],
            '\u{2566}' => vec![
                h!(cy - dv, t),
                h!(cy + dv, t),
                vd!(cx - dl, t),
                vd!(cx + dl, t),
            ],
            '\u{2567}' => vec![h!(cy - dv, t), h!(cy + dv, t), vu!(cx, t)],
            '\u{2568}' => vec![h!(cy, t), vu!(cx - dl, t), vu!(cx + dl, t)],
            '\u{2569}' => vec![
                h!(cy - dv, t),
                h!(cy + dv, t),
                vu!(cx - dl, t),
                vu!(cx + dl, t),
            ],
            '\u{256A}' => vec![h!(cy, t), v!(cx, t)],
            '\u{256B}' => vec![v!(cx - dl, t), v!(cx + dl, t), h!(cy, t)],
            '\u{256C}' => vec![
                v!(cx - dl, t),
                v!(cx + dl, t),
                h!(cy - dv, t),
                h!(cy + dv, t),
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
            // diagonal / quadruple-dash / blocks → fallback font
            _ => vec![],
        }
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
    ) -> (Vec<LayoutRect>, Vec<BatchedTextRun>, Vec<BoxDrawCell>) {
        use itertools::Itertools;
        let mut rects: Vec<LayoutRect> = Vec::new();
        let mut runs: Vec<BatchedTextRun> = Vec::new();
        let mut box_draws: Vec<BoxDrawCell> = Vec::new();
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

                // Box-drawing chars (U+2500–U+257F) — vẽ primitive thay vì
                // rasterize font glyph → pixel-perfect, không anti-alias blur.
                // Chỉ vẽ primitive nếu có geometry; còn lại (diagonal, block
                // shade) fallback font.
                if Self::is_box_drawing(cell.c)
                    && !Self::box_drawing_rects(cell.c, 16, 16).is_empty()
                {
                    if let Some(b) = current_batch.take() {
                        runs.push(b);
                    }
                    box_draws.push(BoxDrawCell {
                        point: lp,
                        color: style.color,
                        c: cell.c,
                    });
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
        }
        if let Some(b) = current_batch {
            runs.push(b);
        }
        (rects, runs, box_draws)
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
        // Snap text origin sang device pixel grid để glyph rasterize khít
        // pixel grid (tránh subpixel blur cho box-drawing / đường kẻ).
        let scale_factor = window.scale_factor().max(1.0);
        let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
        let pos = point(
            px(snap_px(f32::from(
                origin.x + self.start.column as f32 * cell_w,
            ))),
            px(snap_px(f32::from(
                origin.y + self.start.line as f32 * line_h,
            ))),
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
            let raw = cx
                .text_system()
                .advance(font_id, font_px, 'm')
                .map(|s| f32::from(s.width))
                .unwrap_or(8.0);
            px(snap_px(raw))
        };
        let line_height = px(snap_px(f32::from(font_px) * self.line_height_factor));

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

        let (rects, runs, box_draws) = Self::layout_grid(
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
                    let color = self.cursor_color_override.unwrap_or_else(|| {
                        resolve_cell_color(
                            &alacritty_terminal::vte::ansi::Color::Named(NamedColor::Cursor),
                            &self.theme,
                        )
                    });
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
            rects,
            selection_rects,
            runs,
            box_draws,
            cursor,
            background: self.theme.bg,
            cell_width,
            line_height,
            grid_origin,
            gutter_width,
            gutter_entries,
            gutter_bg,
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
            let font_px = self.font_size;

            // Snap helper cho paint — snap tọa độ logical sang device pixel grid.
            let scale_factor = window.scale_factor().max(1.0);
            let snap_px = |value: f32| -> f32 { (value * scale_factor).floor() / scale_factor };
            let ceil_px = |value: f32| -> f32 { (value * scale_factor).ceil() / scale_factor };

            // Cell bg rects — snap x/y/width/height sang device pixel grid.
            for r in &layout.rects {
                let pos = point(
                    px(snap_px(f32::from(origin.x + r.point.column as f32 * cw))),
                    px(snap_px(f32::from(origin.y + r.point.line as f32 * lh))),
                );
                let sz = size(px(ceil_px(f32::from(cw * r.num_cells as f32))), lh);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            }

            // Selection highlight (sau bg rects, trước text để text hiện trên nền).
            for r in &layout.selection_rects {
                let pos = point(
                    px(snap_px(f32::from(origin.x + r.point.column as f32 * cw))),
                    px(snap_px(f32::from(origin.y + r.point.line as f32 * lh))),
                );
                let sz = size(px(ceil_px(f32::from(cw * r.num_cells as f32))), lh);
                window.paint_quad(fill(Bounds::new(pos, sz), r.color));
            }

            // Text runs.
            for run in &layout.runs {
                run.paint(origin, cw, lh, font_px, window, cx);
            }

            // Box-drawing primitive — vẽ bằng fill rects pixel-perfect (như
            // Windows Terminal AtlasEngine) thay vì rasterize font glyph.
            let cw_d = (f32::from(cw) * scale_factor).round() as i32;
            let lh_d = (f32::from(lh) * scale_factor).round() as i32;
            for bd in &layout.box_draws {
                let cell_x_logical = snap_px(f32::from(origin.x + bd.point.column as f32 * cw));
                let cell_y_logical = snap_px(f32::from(origin.y + bd.point.line as f32 * lh));
                for (rx, ry, rw, rh) in Self::box_drawing_rects(bd.c, cw_d, lh_d) {
                    let pos = point(
                        px(cell_x_logical + rx as f32 / scale_factor),
                        px(cell_y_logical + ry as f32 / scale_factor),
                    );
                    let sz = size(px(rw as f32 / scale_factor), px(rh as f32 / scale_factor));
                    window.paint_quad(fill(Bounds::new(pos, sz), bd.color));
                }
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
                        px(snap_px(f32::from(origin.x + cur.point.column as f32 * cw))),
                        px(snap_px(f32::from(origin.y + cur.point.line as f32 * lh))),
                    );
                    let sz = match cur.shape {
                        CursorShape::Beam => {
                            // Thanh dọc hẹp: 20% cell width, full height.
                            let bar_w = (cw * 0.2).max(px(1.0));
                            size(px(ceil_px(f32::from(bar_w))), lh)
                        }
                        CursorShape::Underline => {
                            // Gạch dưới: full width, 15% line height (min 2px).
                            let ul_h = (lh * 0.15).max(px(2.0));
                            size(cw, px(ceil_px(f32::from(ul_h))))
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
