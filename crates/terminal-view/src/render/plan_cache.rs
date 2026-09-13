//! Per-row plan cache, keyed on row identity.
//!
//! `US-0085` replaced the old three-phase selection — damage ∪ cursor row ∪
//! never-built rows, then a per-row content hash to verify the candidates —
//! with one comparison. A `RowKey` is `(RowId, SeqNo)`: the engine stamps a row
//! only when it actually changes, and copies it into this view's render state
//! only when that stamp passed this view's watermark, so
//! `stored_key != frame.row_key(r)` **is** the answer. The hash was there
//! because the old damage escalated to `Full` on every scroll; the render
//! state reports a scroll as a delta instead, so the plans shift with their rows
//! and only the rows that really changed are rebuilt.
//!
//! An `Unchanged` frame returns before any of that: no key scan, no URL scan, no
//! layout.

use oneterm_terminal::RenderUpdate;

use super::diagnostics::FrameStats;
use super::frame::{Frame, GridSize, RowKey};
use super::glyphs::GlyphCache;
use super::row_plan::{PlanContext, RowPlan, Scratch, build_row_plan};
use super::shapes::CellSizeDevicePx;
use crate::url::url_masks_into;

/// Everything besides cell content that changes how a row is planned. A
/// change drops every key so rows rebuild without comparing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct StyleKey {
    pub font_family: u32,
    pub font_size_bits: u32,
    pub weight_bits: u32,
    pub features_hash: u64,
    pub palette_hash: u64,
    pub min_contrast_bits: u32,
    pub semantic_enabled: bool,
    pub shell_profile: u8,
    pub show_gutter: bool,
}

pub(crate) struct PlanCache {
    rows: Vec<RowPlan>,
    /// The row each plan was built for; `None` until it has been built once.
    keys: Vec<Option<RowKey>>,
    dirty: Vec<bool>,
    style: Option<StyleKey>,
    grid: Option<GridSize>,
    mask_prev: Vec<Vec<bool>>,
    mask_cur: Vec<Vec<bool>>,
    wraps: Vec<bool>,
    /// Cell geometry the plans were built with: the device cell size and the
    /// logical cell width (as bits). Shape quads are stored in device pixels
    /// and text runs are shaped with `force_width`, so a scale-factor change —
    /// dragging the window to a monitor with a different DPI — must rebuild
    /// every row even though the grid size and the style key are unchanged
    /// (`grid_size_for` yields the same rows/cols at any scale).
    cell: Option<(CellSizeDevicePx, u32)>,
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanCache {
    pub(crate) fn new() -> Self {
        Self {
            rows: Vec::new(),
            keys: Vec::new(),
            dirty: Vec::new(),
            style: None,
            grid: None,
            mask_prev: Vec::new(),
            mask_cur: Vec::new(),
            wraps: Vec::new(),
            cell: None,
        }
    }

    pub(crate) fn rows(&self) -> &[RowPlan] {
        &self.rows
    }

    #[cfg(test)]
    pub(crate) fn row(&self, r: usize) -> Option<&RowPlan> {
        self.rows.get(r)
    }

    /// The URL mask of display row `r` as of the last update (empty when the
    /// row had no URL or was never scanned).
    #[cfg(test)]
    pub(crate) fn url_mask(&self, r: usize) -> &[bool] {
        self.mask_prev.get(r).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Bring the plans up to date with `frame`.
    pub(crate) fn update(
        &mut self,
        frame: &Frame,
        style_key: StyleKey,
        ctx: &PlanContext<'_>,
        scratch: &mut Scratch,
        glyphs: &mut GlyphCache,
        stats: &mut FrameStats,
    ) {
        let size = frame.size();
        let rows = usize::from(size.rows);
        let cell = (ctx.device, f32::from(ctx.cell_width).to_bits());
        let restyled =
            self.grid != Some(size) || self.style != Some(style_key) || self.cell != Some(cell);

        stats.rows_total = rows as u32;
        if !restyled && frame.update() == RenderUpdate::Unchanged {
            // Nothing moved and nothing was copied: the plans, the masks and
            // the keys all still describe this frame.
            stats.frames_unchanged += 1;
            return;
        }

        if restyled {
            self.rows.resize_with(rows, RowPlan::default);
            self.keys.clear();
            self.keys.resize(rows, None);
        } else {
            if let RenderUpdate::Partial { scrolled } = frame.update() {
                self.shift(scrolled, rows);
            }
            self.rows.resize_with(rows, RowPlan::default);
            self.keys.resize(rows, None);
        }

        // Phase 1: a plan is stale exactly when the row it was built for is no
        // longer the row at that index, or that row has changed since.
        self.dirty.clear();
        self.dirty.resize(rows, false);
        for r in 0..rows {
            self.dirty[r] = self.keys[r].is_none() || self.keys[r] != frame.row_key(r);
        }
        stats.rows_candidate = self.dirty.iter().filter(|&&d| d).count() as u32;

        // Phase 2: a changed row may start or end a wrapped URL, which changes
        // the class of untouched continuation rows (deviation 10).
        let any_dirty = self.dirty.iter().any(|&d| d);
        if any_dirty {
            url_masks_into(frame, &mut self.mask_cur, &mut self.wraps);
            stats.url_scans += 1;
            self.mask_prev.resize_with(rows, Vec::new);
            for (r, d) in self.dirty.iter_mut().enumerate() {
                if self.mask_cur[r] != self.mask_prev[r] {
                    *d = true;
                }
            }
        }

        // Phase 3: rebuild.
        for r in 0..rows {
            if !self.dirty[r] {
                continue;
            }
            let mask: &[bool] = if any_dirty {
                &self.mask_cur[r]
            } else {
                &self.mask_prev[r]
            };
            build_row_plan(
                frame.row(r),
                ctx,
                mask,
                scratch,
                glyphs,
                stats,
                &mut self.rows[r],
            );
            stats.rows_planned += 1;
        }

        if any_dirty {
            std::mem::swap(&mut self.mask_prev, &mut self.mask_cur);
        }
        for r in 0..rows {
            self.keys[r] = frame.row_key(r);
        }
        self.grid = Some(size);
        self.style = Some(style_key);
        self.cell = Some(cell);
    }

    /// Scrolling moves plans with their rows, exactly as the render state moves
    /// the rows themselves; the key comparison then finds the rows that were
    /// shifted in from off-screen (deviation 8).
    fn shift(&mut self, scrolled: i32, rows: usize) {
        let len = self.rows.len().min(self.keys.len());
        if scrolled == 0 || len == 0 || len != rows {
            return;
        }
        let distance = scrolled.unsigned_abs() as usize;
        if distance >= len {
            self.keys.iter_mut().for_each(|key| *key = None);
            return;
        }
        if scrolled > 0 {
            self.rows.rotate_left(distance);
            self.keys.rotate_left(distance);
        } else {
            self.rows.rotate_right(distance);
            self.keys.rotate_right(distance);
        }
        // Masks are recomputed whenever any row is dirty, which a scroll
        // guarantees; rotating them keeps the delta check meaningful.
        if self.mask_prev.len() == len {
            if scrolled > 0 {
                self.mask_prev.rotate_left(distance);
            } else {
                self.mask_prev.rotate_right(distance);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::frame::CellFlags;
    use crate::render::frame::test_support::{FrameBuilder, resnapshot, rewrite_row};
    use crate::render::glyphs::FontSet;
    use crate::render::shapes::CellSizeDevicePx;
    use crate::theme::{TerminalTheme, build_terminal_theme};
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TestAppContext, VisualTestContext, px};

    fn font() -> Font {
        Font {
            family: "Test Mono".into(),
            features: FontFeatures::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }

    fn style_key(font_size: f32) -> StyleKey {
        StyleKey {
            font_family: 1,
            font_size_bits: font_size.to_bits(),
            weight_bits: 400f32.to_bits(),
            features_hash: 0,
            palette_hash: 0,
            min_contrast_bits: 4.5f32.to_bits(),
            semantic_enabled: false,
            shell_profile: 0,
            show_gutter: false,
        }
    }

    struct Harness {
        theme: TerminalTheme,
        fonts: FontSet,
        cache: PlanCache,
        scratch: Scratch,
        glyphs: GlyphCache,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                theme: build_terminal_theme(&gpui_component::Theme::default()),
                fonts: FontSet::new(&font(), px(13.0)),
                cache: PlanCache::new(),
                scratch: Scratch::new(),
                glyphs: GlyphCache::new(),
            }
        }

        fn update(
            &mut self,
            cx: &mut VisualTestContext,
            frame: &Frame,
            key: StyleKey,
        ) -> FrameStats {
            self.update_with_cell(cx, frame, key, CellSizeDevicePx { w: 8, h: 16 }, 8.0)
        }

        fn update_with_cell(
            &mut self,
            cx: &mut VisualTestContext,
            frame: &Frame,
            key: StyleKey,
            device: CellSizeDevicePx,
            cell_width: f32,
        ) -> FrameStats {
            let mut stats = FrameStats::default();
            let Harness {
                theme,
                fonts,
                cache,
                scratch,
                glyphs,
            } = self;
            cx.update(|window, _| {
                let ctx = PlanContext {
                    theme,
                    fonts,
                    font_size: px(13.0),
                    font_weight: 400.0,
                    cell_width: px(cell_width),
                    device,
                    semantic: None,
                    window,
                };
                glyphs.begin_frame();
                cache.update(frame, key, &ctx, scratch, glyphs, &mut stats);
            });
            stats
        }
    }

    fn lines(rows: usize) -> Vec<String> {
        (0..rows).map(|r| format!("line {r} text")).collect()
    }

    fn frame_with(texts: &[String], cols: usize) -> FrameBuilder {
        let mut b = FrameBuilder::new(texts.len(), cols);
        for (r, t) in texts.iter().enumerate() {
            b = b.text(r, 0, t);
        }
        b
    }

    /// The first frame plans every row; a frame with no new output is
    /// `Unchanged` and does nothing at all — no key scan, no URL scan, no
    /// layout. That is the tri-state paying.
    #[gpui::test]
    fn plan_cache_first_frame_plans_every_row_and_idle_plans_none(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(10);
        let (mut frame, mut fixture) = frame_with(&texts, 20).build_with_fixture();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 10);
        assert_eq!(s.rows_candidate, 10);
        assert_eq!(s.url_scans, 1);

        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.frames_unchanged, 1, "the frame reported Unchanged");
        assert_eq!(s.rows_planned, 0);
        assert_eq!(s.rows_candidate, 0, "no row is even considered");
        assert_eq!(s.url_scans, 0);
        assert_eq!(s.shape_calls, 0);
    }

    /// A scroll shifts the plans with their rows: only the rows that came in
    /// from off-screen lose theirs (deviation 8). The engine reports the shift
    /// as a delta, so this no longer costs a full-grid hash.
    #[gpui::test]
    fn scroll_shifts_plans_and_replans_only_scrolled_in_rows(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 20).build_with_fixture();
        // Ten output lines into a five-row viewport: five rows of history.
        fixture
            .feed(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix\r\nseven\r\neight\r\nnine\r\nten");
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 5);

        // Scroll two rows into history: content moves down, two new rows on top.
        fixture.scroll_back(2);
        resnapshot(&mut frame, &mut fixture);
        assert_eq!(frame.update(), RenderUpdate::Partial { scrolled: -2 });
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 2, "only the scrolled-in rows rebuild");
        assert_eq!(s.rows_candidate, 2, "the rest kept their keys");

        // Back down by one.
        fixture.scroll_forward(1);
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 1);

        // A jump of a whole screen has nothing left to shift.
        fixture.scroll_back(5);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        fixture.scroll_forward(5);
        resnapshot(&mut frame, &mut fixture);
        assert_eq!(
            frame.update(),
            RenderUpdate::Full,
            "a scroll the cache cannot absorb is a rebuild"
        );
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 5);
    }

    /// A row that changed replans; its neighbours do not.
    #[gpui::test]
    fn only_the_changed_row_replans(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        let (mut frame, mut fixture) = frame_with(&texts, 20).build_with_fixture();
        h.update(cx, &frame, style_key(13.0));

        rewrite_row(&mut frame, &mut fixture, 2, "echoed input");
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_candidate, 1);
        assert_eq!(s.rows_planned, 1);
    }

    /// A selection is not row content: it never invalidates a plan, and the
    /// engine reports it as a `Partial` with nothing changed.
    #[gpui::test]
    fn selection_change_does_not_replan(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        let (mut frame, mut fixture) = frame_with(&texts, 20).build_with_fixture();
        h.update(cx, &frame, style_key(13.0));

        fixture.select((1, 0), (2, 4), oneterm_terminal::SelectionKind::Simple);
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert!(frame.selection().is_some());
        assert_eq!(s.rows_planned, 0, "the rows are untouched");
        assert_eq!(s.rows_candidate, 0);
        assert_eq!(s.url_scans, 0, "no changed row, no URL scan");
    }

    #[gpui::test]
    fn style_key_change_replans_all(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        let (mut frame, mut fixture) = frame_with(&texts, 20).build_with_fixture();
        h.update(cx, &frame, style_key(13.0));
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(14.0));
        assert_eq!(s.rows_planned, 5);
    }

    /// The settings font weight is part of the style key, so shapes (whose
    /// stroke thickness follows the weight) are replanned when it changes.
    #[gpui::test]
    fn weight_change_replans_all(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        let (mut frame, mut fixture) = frame_with(&texts, 20).build_with_fixture();
        h.update(cx, &frame, style_key(13.0));
        resnapshot(&mut frame, &mut fixture);
        let key = StyleKey {
            weight_bits: 700f32.to_bits(),
            ..style_key(13.0)
        };
        let s = h.update(cx, &frame, key);
        assert_eq!(s.rows_planned, 5);
        let s = h.update(cx, &frame, key);
        assert_eq!(s.rows_planned, 0, "the new weight is remembered");
    }

    #[gpui::test]
    fn resize_replans_all_and_resizes_session(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        h.update(cx, &frame_with(&texts, 20).build(), style_key(13.0));
        let mut bigger = texts.clone();
        bigger.push("row 5".into());
        let s = h.update(cx, &frame_with(&bigger, 30).build(), style_key(13.0));
        assert_eq!(s.rows_planned, 6);
        assert_eq!(h.cache.rows().len(), 6);
        let s = h.update(cx, &frame_with(&texts[..3], 30).build(), style_key(13.0));
        assert_eq!(h.cache.rows().len(), 3);
        assert_eq!(s.rows_planned, 3);
    }

    /// Regression: a scale-factor change (the window dragged to a monitor with
    /// a different DPI) keeps the grid size and the style key, but the device
    /// cell size changes and shape quads are cached in device pixels.
    #[gpui::test]
    fn device_cell_size_change_replans_all(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(4, 10)
            .text(0, 0, "██ ok")
            .build_with_fixture();
        h.update_with_cell(
            cx,
            &frame,
            style_key(13.0),
            CellSizeDevicePx { w: 8, h: 16 },
            8.0,
        );
        assert_eq!(h.cache.rows[0].shapes[0].rect.w, 16, "two 8 px blocks");
        resnapshot(&mut frame, &mut fixture);
        let s = h.update_with_cell(
            cx,
            &frame,
            style_key(13.0),
            CellSizeDevicePx { w: 16, h: 32 },
            8.0,
        );
        assert_eq!(s.rows_planned, 4, "every row rebuilds at the new cell size");
        assert_eq!(
            h.cache.rows[0].shapes[0].rect.w, 32,
            "quads follow the cell size"
        );
    }

    #[gpui::test]
    fn url_mask_delta_replans_continuation_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (frame, _fixture) = FrameBuilder::new(3, 10)
            .text(0, 0, "https://x.")
            .text(1, 0, "com/path")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));
        assert!(h.cache.url_mask(1).is_empty() || !h.cache.url_mask(1)[0]);

        // Only row 0 changes (it now wraps), but row 1 becomes a URL
        // continuation and must replan too.
        let (wrapped, _fixture) = FrameBuilder::new(3, 10)
            .text(0, 0, "https://x.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "com/path")
            .build_with_fixture();
        let mut h = Harness::new();
        h.update(cx, &wrapped, style_key(13.0));
        assert!(h.cache.url_mask(1)[0]);
        assert_eq!(h.cache.row(1).map(|p| p.decorations.len()), Some(1));
    }
}
