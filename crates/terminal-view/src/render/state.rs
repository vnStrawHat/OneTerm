//! Cross-frame render state shared by the view, the element and the input
//! handlers (HLD "Cross-frame State and Caches").
//!
//! `RenderInputs` carries everything the element needs besides the session, so
//! the element is testable with `FakeTerminalSession` and no view. The view
//! refreshes the inputs before building the element; the element writes
//! `geometry` (the hit-test contract) and the per-frame overlays.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::rc::Rc;

use gpui::{App, Edges, Font, Pixels, ShapedLine, Window, px};

use super::cursor::{self, CursorConfig, CursorPaint};
use super::diagnostics::FrameStats;
#[cfg(any(test, feature = "terminal-diagnostics"))]
use super::diagnostics::LatencySamples;
use super::frame::{Fnv1a, Frame, GridSize};
use super::glyphs::{FontSet, GlyphCache};
use super::graphics::GraphicStore;
use super::metrics::{CellMetrics, GridGeometry, measure};
use super::overlay::{RowSpan, SearchHighlight, SearchRect, search_rects, selection_rects};
use super::plan_cache::{PlanCache, StyleKey};
use super::row_plan::{PlanContext, Scratch};
use crate::highlight::SemanticOverlay;
use crate::theme::TerminalTheme;

/// Seconds since midnight; the view stamps one per output line.
pub(crate) type SecondsOfDay = u32;

/// Placeholder for rows without a known timestamp.
const NO_TIME: &str = "--:--:--";
/// `[HH:MM:SS] ` — bytes painted in the clock color.
const CLOCK_LEN: usize = 11;
/// Space between the gutter text and the grid, and left inset of the labels.
const GUTTER_PAD: f32 = 8.0;

/// Per-line timestamps for the gutter (owned by the view, grow-only).
#[derive(Clone, Default)]
pub(crate) struct GutterInputs {
    /// `times[j]` belongs to the line with absolute index `base + j`.
    pub times: Rc<VecDeque<SecondsOfDay>>,
    pub base: usize,
    /// Total lines output so far (monotonic).
    pub absolute_line_count: usize,
}

/// Everything the element needs besides the session.
pub(crate) struct RenderInputs {
    pub theme: Rc<TerminalTheme>,
    pub font: Font,
    pub font_size: Pixels,
    pub line_height_factor: f32,
    pub cell_width_override: Option<f32>,
    pub padding: Edges<Pixels>,
    pub show_gutter: bool,
    pub cursor: CursorConfig,
    pub gutter: GutterInputs,
    /// Viewport-clamped search matches; the view refills it in place.
    pub search: Vec<SearchHighlight>,
    pub semantic: SemanticOverlay,
    /// Pointer over a URL: the mouse cursor becomes a hand.
    pub url_hovering: bool,
}

impl RenderInputs {
    pub(crate) fn new(theme: Rc<TerminalTheme>, font: Font, font_size: Pixels) -> Self {
        Self {
            theme,
            font,
            font_size,
            line_height_factor: 1.2,
            cell_width_override: None,
            padding: Edges::default(),
            show_gutter: false,
            cursor: CursorConfig {
                shape: None,
                color: None,
                focused: false,
                blink_visible: true,
            },
            gutter: GutterInputs::default(),
            search: Vec::new(),
            semantic: SemanticOverlay::new(oneterm_highlight::ShellProfile::Unix, false),
            url_hovering: false,
        }
    }

    pub(crate) fn style_key(&self) -> StyleKey {
        let mut family = Fnv1a::new();
        family.write(self.font.family.as_bytes());
        let mut features = Fnv1a::new();
        for (name, value) in self.font.features.0.iter() {
            features.write(name.as_bytes());
            features.write_u32(*value);
        }
        StyleKey {
            font_family: family.finish() as u32,
            font_size_bits: f32::from(self.font_size).to_bits(),
            weight_bits: self.font.weight.0.to_bits(),
            features_hash: features.finish(),
            palette_hash: self.theme.colors.hash,
            min_contrast_bits: self.theme.min_contrast.to_bits(),
            semantic_enabled: self.semantic.is_enabled(),
            shell_profile: self.semantic.profile() as u8,
            show_gutter: self.show_gutter,
        }
    }
}

/// Selection and search spans of the current frame.
#[derive(Default)]
pub(crate) struct Overlays {
    pub selection: Vec<RowSpan>,
    pub search: Vec<SearchRect>,
}

/// One shaped gutter label.
pub(crate) struct GutterLabel {
    pub line: ShapedLine,
    /// Bytes painted in `clock_fg`; the rest in `line_number_fg`.
    pub clock_len: usize,
}

#[derive(Default)]
pub(crate) struct GutterLabels {
    pub labels: Vec<GutterLabel>,
    pub width: Pixels,
}

/// The inputs `CellMetrics` were measured from.
#[derive(Clone, PartialEq)]
struct MetricsKey {
    font: Font,
    font_size: Pixels,
    line_height_factor: f32,
    cell_width_override: Option<f32>,
    scale_factor: f32,
}

pub(crate) struct RenderState {
    pub frame: Frame,
    pub plans: PlanCache,
    pub glyphs: GlyphCache,
    /// Written in prepaint, read by input handlers.
    pub geometry: Option<GridGeometry>,
    pub inputs: RenderInputs,
    pub overlays: Overlays,
    pub scratch: Scratch,
    pub gutter: GutterLabels,
    /// The grid last pushed to `session.resize`.
    pub last_grid: Option<GridSize>,
    /// The cell size last pushed to `session.set_cell_size`.
    pub last_cell_size: Option<(u16, u16)>,
    /// Sixel images by id, GPU-ready.
    pub graphics: GraphicStore,
    pub stats: FrameStats,
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub latency: LatencySamples,
    #[cfg(feature = "terminal-diagnostics")]
    pub log: super::diagnostics::DiagnosticsLog,
    /// Font variants for `fonts_key`; refreshed by [`Self::ensure_fonts`].
    pub fonts: FontSet,
    fonts_key: (Font, Pixels),
    metrics: Option<(MetricsKey, CellMetrics)>,
}

impl RenderState {
    pub(crate) fn new(inputs: RenderInputs) -> Self {
        let fonts = FontSet::new(&inputs.font, inputs.font_size);
        let fonts_key = (inputs.font.clone(), inputs.font_size);
        Self {
            frame: Frame::new(),
            plans: PlanCache::new(),
            glyphs: GlyphCache::new(),
            geometry: None,
            inputs,
            overlays: Overlays::default(),
            scratch: Scratch::new(),
            gutter: GutterLabels::default(),
            last_grid: None,
            last_cell_size: None,
            graphics: GraphicStore::default(),
            stats: FrameStats::default(),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            latency: LatencySamples::new(),
            #[cfg(feature = "terminal-diagnostics")]
            log: super::diagnostics::DiagnosticsLog::new(),
            fonts,
            fonts_key,
            metrics: None,
        }
    }

    /// Rebuild the font variants when the input font or size changed. The
    /// shaped-run cache goes with them: its key covers only family, size,
    /// weight and slant, so entries shaped with the old features or fallbacks
    /// would otherwise be handed back forever.
    pub(crate) fn ensure_fonts(&mut self) {
        if self.fonts_key.0 == self.inputs.font && self.fonts_key.1 == self.inputs.font_size {
            return;
        }
        self.fonts = FontSet::new(&self.inputs.font, self.inputs.font_size);
        self.fonts_key = (self.inputs.font.clone(), self.inputs.font_size);
        self.glyphs.clear();
    }

    /// Cell metrics for the current inputs and window scale, re-measured only
    /// when one of them changed (the measurement is a platform call).
    pub(crate) fn metrics(&mut self, window: &Window, cx: &App) -> CellMetrics {
        let key = MetricsKey {
            font: self.inputs.font.clone(),
            font_size: self.inputs.font_size,
            line_height_factor: self.inputs.line_height_factor,
            cell_width_override: self.inputs.cell_width_override,
            scale_factor: window.scale_factor(),
        };
        if let Some((cached, metrics)) = &self.metrics
            && *cached == key
        {
            return *metrics;
        }
        let metrics = measure(
            &key.font,
            key.font_size,
            key.line_height_factor,
            key.cell_width_override,
            window,
            cx,
        );
        self.metrics = Some((key, metrics));
        metrics
    }

    /// Rebuild the plans of the rows that changed in the snapshotted frame.
    pub(crate) fn update_plans(&mut self, metrics: &CellMetrics, window: &Window) {
        self.ensure_fonts();
        let style_key = self.inputs.style_key();
        let Self {
            frame,
            plans,
            glyphs,
            inputs,
            scratch,
            stats,
            fonts,
            ..
        } = self;
        let ctx = PlanContext {
            theme: &inputs.theme,
            fonts,
            font_size: inputs.font_size,
            font_weight: inputs.font.weight.0,
            cell_width: metrics.cell_width,
            device: metrics.device,
            semantic: inputs.semantic.is_enabled().then_some(&inputs.semantic),
            window,
        };
        plans.update(frame, style_key, &ctx, scratch, glyphs, stats);
    }

    /// Selection and search spans for the snapshotted frame.
    pub(crate) fn compute_overlays(&mut self) {
        let size = self.frame.size();
        self.overlays.selection.clear();
        if let Some(selection) = self.frame.selection() {
            selection_rects(selection, size, &mut self.overlays.selection);
        }
        search_rects(&self.inputs.search, size, &mut self.overlays.search);
    }

    /// The cursor to paint this frame, if any.
    pub(crate) fn resolve_cursor(
        &mut self,
        geometry: &GridGeometry,
        window: &Window,
    ) -> Option<CursorPaint> {
        let resolved = cursor::resolve(&self.frame, &self.inputs.cursor)?;
        self.ensure_fonts();
        let Self {
            frame,
            glyphs,
            inputs,
            scratch,
            stats,
            fonts,
            ..
        } = self;
        Some(CursorPaint::build(
            resolved,
            frame,
            &inputs.cursor,
            &inputs.theme,
            geometry,
            fonts,
            inputs.font_size,
            glyphs,
            &mut scratch.label,
            window,
            stats,
        ))
    }

    /// Width of the gutter column for the current line count, or zero.
    pub(crate) fn gutter_width(&mut self, window: &Window) -> Pixels {
        if !self.inputs.show_gutter {
            self.gutter.width = px(0.0);
            return self.gutter.width;
        }
        let digits = gutter_digits(self.inputs.gutter.absolute_line_count);
        let font_size = self.inputs.font_size;
        self.ensure_fonts();
        let Self {
            scratch,
            glyphs,
            stats,
            fonts,
            ..
        } = self;
        let (font, key) = fonts.regular();
        scratch.label.clear();
        scratch.label.push_str("[00:00:00] ");
        for _ in 0..digits {
            scratch.label.push('0');
        }
        let line = glyphs.shape(&scratch.label, font, key, font_size, None, window, stats);
        self.gutter.width = line.width() + px(GUTTER_PAD);
        self.gutter.width
    }

    /// Format and shape one label per display row (parity items 24, 25).
    pub(crate) fn build_gutter_labels(
        &mut self,
        rows: usize,
        display_offset: usize,
        window: &Window,
    ) {
        self.gutter.labels.clear();
        if !self.inputs.show_gutter {
            return;
        }
        let font_size = self.inputs.font_size;
        self.ensure_fonts();
        let Self {
            scratch,
            glyphs,
            stats,
            fonts,
            inputs,
            gutter,
            ..
        } = self;
        let (font, key) = fonts.regular();
        let layout = GutterLayout {
            base: inputs.gutter.base,
            absolute_line_count: inputs.gutter.absolute_line_count,
            display_offset,
            viewport_lines: rows,
        };
        for i in 0..rows {
            let clock_len =
                format_gutter_label(&mut scratch.label, &inputs.gutter.times, &layout, i);
            let line = glyphs.shape(&scratch.label, font, key, font_size, None, window, stats);
            gutter.labels.push(GutterLabel { line, clock_len });
        }
    }

    /// Left inset of gutter labels inside the element.
    pub(crate) fn gutter_inset() -> Pixels {
        px(GUTTER_PAD / 2.0)
    }
}

/// Digits reserved for the line-number column: enough for the absolute line
/// count (monotonic, survives scrollback eviction), never fewer than two.
pub(crate) fn gutter_digits(absolute_line_count: usize) -> usize {
    let mut n = absolute_line_count.max(1);
    let mut digits = 0;
    while n > 0 {
        digits += 1;
        n /= 10;
    }
    digits.max(2)
}

/// Which absolute lines the display rows show.
pub(crate) struct GutterLayout {
    pub base: usize,
    pub absolute_line_count: usize,
    pub display_offset: usize,
    pub viewport_lines: usize,
}

/// Write the label of display row `i` into `out` and return its clock length.
///
/// A line is looked up by its absolute index (line number − 1). A line with
/// content but no stamp yet (read skew after `clear`) reuses the newest stamp;
/// a line older than the tracked region reuses the oldest; `[--:--:--]` is
/// reserved for rows above line 1 or when nothing was stamped.
pub(crate) fn format_gutter_label(
    out: &mut String,
    times: &VecDeque<SecondsOfDay>,
    layout: &GutterLayout,
    i: usize,
) -> usize {
    let digits = gutter_digits(layout.absolute_line_count);
    let abs_index = layout.absolute_line_count as i64
        - layout.display_offset as i64
        - layout.viewport_lines as i64
        + i as i64;
    let line_num = (abs_index + 1).max(1);
    let time = if abs_index < 0 {
        None
    } else {
        let ai = abs_index as usize;
        if ai >= layout.base {
            times
                .get(ai - layout.base)
                .or_else(|| times.back())
                .copied()
        } else {
            times.front().copied()
        }
    };
    out.clear();
    out.push('[');
    match time {
        Some(t) => {
            let (h, m, s) = (t / 3600 % 24, t / 60 % 60, t % 60);
            // Writing into a `String` cannot fail.
            let _ = write!(out, "{h:02}:{m:02}:{s:02}");
        }
        None => out.push_str(NO_TIME),
    }
    out.push_str("] ");
    let _ = write!(out, "{line_num:>digits$}");
    CLOCK_LEN
}

#[cfg(test)]
mod tests {
    use gpui::{FontFeatures, FontStyle, FontWeight};

    use super::*;
    use crate::render::diagnostics::FrameStats;
    use crate::theme::build_terminal_theme;

    /// Regression: `FontKey` does not cover `Font::features`, so a features
    /// change must drop the shaped-run cache instead of serving runs shaped
    /// with the old feature set.
    #[gpui::test]
    fn font_change_clears_the_shaped_run_cache(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let font = Font {
                family: "Test Mono".into(),
                features: FontFeatures::default(),
                fallbacks: None,
                weight: FontWeight::NORMAL,
                style: FontStyle::Normal,
            };
            let theme = Rc::new(build_terminal_theme(&gpui_component::Theme::default()));
            let mut state = RenderState::new(RenderInputs::new(theme, font.clone(), px(13.0)));
            let mut stats = FrameStats::default();
            let (f, key) = state.fonts.regular();
            let f = f.clone();
            state
                .glyphs
                .shape("cached", &f, key, px(13.0), None, window, &mut stats);
            assert_eq!(state.glyphs.len(), 1);

            // Same family, size, weight and slant: the run key is unchanged.
            state.ensure_fonts();
            assert_eq!(state.glyphs.len(), 1, "an unchanged font keeps the cache");

            state.inputs.font.features = FontFeatures::disable_ligatures();
            state.ensure_fonts();
            assert_eq!(state.glyphs.len(), 0, "a features change drops the cache");
        });
    }

    fn times(list: &[&str]) -> VecDeque<u32> {
        list.iter()
            .map(|s| {
                let mut parts = s.split(':').map(|p| p.parse::<u32>().unwrap_or(0));
                let h = parts.next().unwrap_or(0);
                let m = parts.next().unwrap_or(0);
                let sec = parts.next().unwrap_or(0);
                h * 3600 + m * 60 + sec
            })
            .collect()
    }

    fn labels(
        list: &[&str],
        base: usize,
        absolute: usize,
        offset: usize,
        viewport: usize,
    ) -> Vec<String> {
        let t = times(list);
        let layout = GutterLayout {
            base,
            absolute_line_count: absolute,
            display_offset: offset,
            viewport_lines: viewport,
        };
        let mut out = String::new();
        (0..viewport)
            .map(|i| {
                let clock = format_gutter_label(&mut out, &t, &layout, i);
                assert_eq!(clock, CLOCK_LEN);
                out.clone()
            })
            .collect()
    }

    #[test]
    fn gutter_labels_use_fallbacks() {
        assert_eq!(
            labels(&["10:00:01", "10:00:02", "10:00:03"], 0, 3, 0, 3),
            vec!["[10:00:01]  1", "[10:00:02]  2", "[10:00:03]  3"]
        );
        // Rows above line 1 show the placeholder.
        assert_eq!(labels(&["10:00:01"], 0, 1, 0, 3)[0], "[--:--:--]  1");
        // Newer lines than the stamps reuse the newest stamp.
        let out = labels(&["10:00:01", "10:00:02", "10:00:03"], 0, 5, 0, 5);
        assert_eq!(out[3], "[10:00:03]  4");
        assert_eq!(out[4], "[10:00:03]  5");
        // Lines older than the tracked region reuse the oldest stamp.
        assert_eq!(
            labels(&["10:00:10", "10:00:11"], 9, 12, 1, 3),
            vec!["[10:00:10]  9", "[10:00:10] 10", "[10:00:11] 11"]
        );
        assert_eq!(labels(&[], 0, 0, 0, 1)[0], "[--:--:--]  1");
    }

    #[test]
    fn gutter_digits_grow_with_the_absolute_count() {
        assert_eq!(gutter_digits(0), 2);
        assert_eq!(gutter_digits(9), 2);
        assert_eq!(gutter_digits(150), 3);
        assert_eq!(gutter_digits(10_000), 5);
    }
}
