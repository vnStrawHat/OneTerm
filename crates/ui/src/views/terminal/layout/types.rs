//! Core layout types for `TerminalElement`.

use gpui::{Bounds, Hsla, Pixels, Point, SharedString, TextRun};

/// Grid metrics after layout — read by the View to convert mouse pixels → (row, col).
#[derive(Clone, Copy, Default)]
#[allow(dead_code)]
pub(crate) struct GridMetrics {
    pub bounds: Option<Bounds<Pixels>>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Width of the gutter (time + line number) on the left of the terminal.
    pub gutter_width: Pixels,
    /// Top padding above the grid (snapped).
    pub pad_top: Pixels,
    /// Origin of the cell grid (after gutter + pad_left + pad_top, snapped).
    pub grid_origin: Point<Pixels>,
    /// Number of rows and columns in the grid.
    pub rows: usize,
    pub cols: usize,
}

/// Layout point (display line/col, 0-based from top of viewport).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct LayoutPoint {
    pub line: i32,
    pub column: i32,
}

/// Background rect (one line, horizontal batch).
#[derive(Clone, Debug)]
pub(crate) struct LayoutRect {
    pub point: LayoutPoint,
    pub num_cells: usize,
    pub color: Hsla,
}

/// Batched text run (consecutive cells with the same style on the same line).
pub(crate) struct BatchedTextRun {
    pub start: LayoutPoint,
    pub text: String,
    pub cell_count: usize,
    pub style: TextRun,
}

/// One gutter line: text + pixel position (top-left) + byte length of the clock part.
pub(crate) struct GutterEntry {
    pub text: SharedString,
    /// Byte length of the clock part "[HH:MM:SS] " (excluding the line number).
    pub clock_len: usize,
    pub y: Pixels,
}

/// Layout info computed in prepaint → paint.
pub(crate) struct LayoutState {
    pub selection_rects: Vec<LayoutRect>,
    /// Search highlight rects (display coordinates). Painted under the text,
    /// above the cell background.
    pub search_rects: Vec<LayoutRect>,
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

/// Cursor to paint.
pub(crate) struct CursorPaint {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub shape: alacritty_terminal::vte::ansi::CursorShape,
}

/// A box-drawing cell that will be drawn with a primitive fill.
pub(crate) struct BoxDrawCell {
    pub point: LayoutPoint,
    pub color: Hsla,
    pub c: char,
    /// Number of horizontally-adjacent cells this run covers. `1` for all glyphs
    /// except full-width band blocks (`is_full_width_band`), where consecutive
    /// same-glyph/same-colour cells are coalesced into one stretched rect.
    pub num_cells: usize,
}

/// Layout artifacts for one display row — cached across frames.
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

/// Per-frame render stats — count paint calls to find bottlenecks.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct FrameStats {
    pub total_lines: usize,
    pub dirty_lines: usize,
    pub row_layout_calls: usize,
    pub paint_quad_calls: usize,
    pub shape_line_calls: usize,
    pub text_run_paints: usize,
    pub bg_rect_count: usize,
    pub hash_calls: usize,
    pub allocation_buffer_sites: usize,
    pub frame_count: u64,
    /// Wall-clock time of the prepaint phase (layout + shaping + snapshot), µs.
    pub prepaint_us: u128,
    /// Wall-clock time spent in `terminal_info`, including backend locking, µs.
    pub terminal_info_us: u128,
    /// Wall-clock time spent cloning the render snapshot under the backend lock, µs.
    pub snapshot_us: u128,
    /// Wall-clock time of the paint phase (quad emission), µs.
    pub paint_us: u128,
}

/// Per-row layout cache — persisted across frames.
pub(crate) struct RowLayoutCache {
    pub rows: Vec<RowLayout>,
    pub prev_grid_size: Option<(u16, u16)>,
    pub prev_display_offset: usize,
    pub prev_selection: Option<alacritty_terminal::selection::SelectionRange>,
    pub stats: FrameStats,
}

impl RowLayoutCache {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            prev_grid_size: None,
            prev_display_offset: 0,
            prev_selection: None,
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
