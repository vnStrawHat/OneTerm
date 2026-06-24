//! Core layout types cho `TerminalElement`.

use gpui::{Bounds, Hsla, Pixels, SharedString, TextRun};

/// Metrics grid sau layout — View đọc để convert mouse pixel → (row,col).
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
pub(crate) struct LayoutPoint {
    pub line: i32,
    pub column: i32,
}

/// Rect nền (1 dòng, batch ngang).
#[derive(Clone, Debug)]
pub(crate) struct LayoutRect {
    pub point: LayoutPoint,
    pub num_cells: usize,
    pub color: Hsla,
}

/// Text run batch (các cell liên tiếp cùng style, cùng dòng).
pub(crate) struct BatchedTextRun {
    pub start: LayoutPoint,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
}

/// Một dòng gutter: text + vị trí pixel (top-left) + byte length của phần clock.
pub(crate) struct GutterEntry {
    pub text: SharedString,
    /// Byte length của phần clock "[HH:MM:SS] " (không bao gồm line number).
    pub clock_len: usize,
    pub y: Pixels,
}

/// Thông tin layout computed ở prepaint → paint.
pub(crate) struct LayoutState {
    pub selection_rects: Vec<LayoutRect>,
    pub cursor: Option<CursorPaint>,
    pub background: Hsla,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub grid_origin: gpui::Point<Pixels>,
    pub gutter_width: Pixels,
    pub gutter_entries: Vec<GutterEntry>,
    pub gutter_bg: Hsla,
    pub num_lines: usize,
}

/// Con trỏ để paint.
pub(crate) struct CursorPaint {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub shape: alacritty_terminal::vte::ansi::CursorShape,
}

/// Một cell box-drawing sẽ được vẽ bằng primitive fill.
pub(crate) struct BoxDrawCell {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub c: char,
}

/// Layout artifacts cho 1 display row — cached qua frames.
pub(crate) struct RowLayout {
    pub rects: Vec<LayoutRect>,
    pub runs: Vec<BatchedTextRun>,
    pub box_draws: Vec<BoxDrawCell>,
    pub shaped_lines: Vec<Option<gpui::ShapedLine>>,
    pub prev_hash: u64,
}

impl RowLayout {
    pub(crate) fn empty() -> Self {
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
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct FrameStats {
    pub total_lines: usize,
    pub dirty_lines: usize,
    pub paint_quad_calls: usize,
    pub shape_line_calls: usize,
    pub text_run_paints: usize,
    pub bg_rect_count: usize,
    pub hash_calls: usize,
    pub frame_count: u64,
}

/// Per-row layout cache — persisted qua frames.
pub(crate) struct RowLayoutCache {
    pub rows: Vec<RowLayout>,
    pub prev_grid_size: Option<(u16, u16)>,
    pub prev_display_offset: usize,
    pub prev_selection: Option<alacritty_terminal::selection::SelectionRange>,
    pub prev_hovered_url: Option<super::super::url::DetectedUrl>,
    pub prev_ctrl_held: bool,
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

    pub(crate) fn ensure_size(&mut self, num_lines: usize) {
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
