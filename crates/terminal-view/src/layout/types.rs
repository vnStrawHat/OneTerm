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

/// Key inputs that affect row layout output. If any of these change,
/// every cached row must be invalidated (re-layout + re-shape).
///
/// Captures font identity, theme palette, min-contrast, and semantic
/// overlay state. `class_styles` is loaded from a static asset and never
/// changes at runtime, so it is excluded.
#[derive(Clone, PartialEq)]
pub(crate) struct RenderStyleKey {
    /// Font family, weight, style, features (affects text shaping).
    pub font: gpui::Font,
    /// Font size in device pixels (affects shaping + cell metrics).
    pub font_size_bits: u32,
    /// Terminal palette (affects fg/bg color resolution in `layout_row`).
    pub palette: oneterm_terminal::TerminalPalette,
    /// Minimum contrast threshold (affects color correction).
    pub min_contrast_bits: u32,
    /// Whether semantic highlighting is enabled.
    pub semantic_enabled: bool,
    /// Shell profile for the semantic scanner.
    pub shell_profile: oneterm_highlight::ShellProfile,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
const LATENCY_SAMPLE_CAPACITY: usize = 512;

/// Rolling terminal-frame latency samples used only by opt-in diagnostics.
#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Default)]
pub(crate) struct LatencySamples {
    snapshot_us: std::collections::VecDeque<u128>,
    frame_us: std::collections::VecDeque<u128>,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
impl LatencySamples {
    pub(crate) fn record(&mut self, snapshot_us: u128, frame_us: u128) {
        Self::push_bounded(&mut self.snapshot_us, snapshot_us);
        Self::push_bounded(&mut self.frame_us, frame_us);
    }

    pub(crate) fn len(&self) -> usize {
        self.frame_us.len()
    }

    pub(crate) fn snapshot_percentile(&self, percentile: f64) -> u128 {
        Self::percentile(&self.snapshot_us, percentile)
    }

    pub(crate) fn frame_percentile(&self, percentile: f64) -> u128 {
        Self::percentile(&self.frame_us, percentile)
    }

    fn push_bounded(samples: &mut std::collections::VecDeque<u128>, value: u128) {
        if samples.len() == LATENCY_SAMPLE_CAPACITY {
            samples.pop_front();
        }
        samples.push_back(value);
    }

    fn percentile(samples: &std::collections::VecDeque<u128>, percentile: f64) -> u128 {
        let mut sorted: Vec<_> = samples.iter().copied().collect();
        sorted.sort_unstable();
        sorted
            .get(((sorted.len().saturating_sub(1)) as f64 * percentile).ceil() as usize)
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod latency_tests {
    use super::*;

    #[test]
    fn rolling_percentiles_are_bounded_and_ordered() {
        let mut samples = LatencySamples::default();
        for value in 1..=LATENCY_SAMPLE_CAPACITY as u128 + 10 {
            samples.record(value, value * 2);
        }
        assert_eq!(samples.len(), LATENCY_SAMPLE_CAPACITY);
        assert!(samples.snapshot_percentile(0.95) <= samples.snapshot_percentile(0.99));
        assert!(samples.frame_percentile(0.95) <= samples.frame_percentile(0.99));
    }
}

/// Per-row layout cache — persisted across frames.
pub(crate) struct RowLayoutCache {
    pub rows: Vec<RowLayout>,
    pub prev_grid_size: Option<(u16, u16)>,
    pub prev_display_offset: usize,
    pub prev_style_key: Option<RenderStyleKey>,
    /// Cached URL masks from the last frame with dirty rows (PERF-09).
    /// Reused when the terminal is idle to avoid scanning all cells.
    pub cached_url_masks: Vec<Vec<bool>>,
    /// Rolling p95/p99 source data; omitted from normal production builds.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub latency_samples: LatencySamples,
    /// Last renderer diagnostic report; omitted unless diagnostics are enabled.
    #[cfg(feature = "terminal-diagnostics")]
    pub diagnostics_last_report: Option<std::time::Instant>,
    pub stats: FrameStats,
}

impl RowLayoutCache {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            prev_grid_size: None,
            prev_display_offset: 0,
            prev_style_key: None,
            cached_url_masks: Vec::new(),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            latency_samples: LatencySamples::default(),
            #[cfg(feature = "terminal-diagnostics")]
            diagnostics_last_report: None,
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

/// Bundle of render cache state — persisted across frames on `LocalTerminalView`,
/// passed to prepaint as a single unit (ARCH-06).
pub(crate) struct TerminalRenderCache {
    pub row_cache: std::rc::Rc<std::cell::RefCell<RowLayoutCache>>,
    pub cached_gutter:
        std::rc::Rc<std::cell::RefCell<Option<(Pixels, usize, Pixels, SharedString)>>>,
    pub last_grid_size: std::rc::Rc<std::cell::RefCell<Option<(u16, u16)>>>,
    pub metrics: std::rc::Rc<std::cell::RefCell<GridMetrics>>,
}
