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
use crate::url::{fill_wraps, url_masks_rows_into};

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
    /// The masks as of the last update — authoritative for every row, whether
    /// or not this frame rescanned it.
    mask_prev: Vec<Vec<bool>>,
    /// Scratch for the rows this frame rescans; swapped row by row into
    /// `mask_prev` so both keep their inner allocations.
    mask_cur: Vec<Vec<bool>>,
    /// This frame's per-row `WRAPLINE` flags.
    wraps: Vec<bool>,
    /// The previous frame's, aligned with `mask_prev` (so `shift` rotates it
    /// too). A row whose wrap flag was *dropped* still has to pull its old
    /// continuation row into the rescan, so the run walk uses the union.
    wraps_prev: Vec<bool>,
    /// The rows this frame rescans: the dirty rows closed under wrap runs.
    scan: Vec<bool>,
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
            wraps_prev: Vec::new(),
            scan: Vec::new(),
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
        // the class of untouched continuation rows (deviation 10). That hazard
        // is bounded, and the bound is the scope of the rescan: a URL can only
        // reach a row it is wrap-connected to, so the answer is the dirty rows
        // closed under the wrap runs `self.wraps` already tracks — never the
        // whole viewport (`US-0092`). Rows outside those runs keep the masks
        // they had, which is why `mask_prev` stays authoritative for all rows.
        //
        // One dependency is **not** a row's content: where the viewport's top
        // edge cuts a wrap run. Display row 0's mask is extended into it from
        // the row above only while that row is on screen, and a scroll moves the
        // edge without changing any row's `(RowId, SeqNo)` — so a URL whose head
        // scrolls above the top leaves a stale underline behind
        // (`US-0092` verification, defect 2). The seam is therefore rescanned on
        // every scrolled frame: one extra row, closed under its run like any
        // other, and it lands in `self.scan` rather than `self.dirty` so the
        // mask-delta compare still decides whether a plan is rebuilt.
        let scrolled_seam =
            matches!(frame.update(), RenderUpdate::Partial { scrolled } if scrolled != 0);
        let any_dirty = self.dirty.iter().any(|&d| d);
        if any_dirty {
            fill_wraps(frame, &mut self.wraps);
            self.wraps_prev.resize(rows, false);
            self.mask_prev.resize_with(rows, Vec::new);
            self.mask_cur.resize_with(rows, Vec::new);
            self.mark_scan_runs(rows, scrolled_seam);
            stats.url_scans += 1;

            let mut r = 0;
            while r < rows {
                if !self.scan[r] {
                    r += 1;
                    continue;
                }
                let start = r;
                while r < rows && self.scan[r] {
                    r += 1;
                }
                stats.url_rows_scanned += (r - start) as u32;
                url_masks_rows_into(frame, &mut self.mask_cur, &self.wraps, start..r);
                for row in start..r {
                    if self.mask_cur[row] != self.mask_prev[row] {
                        self.dirty[row] = true;
                    }
                    std::mem::swap(&mut self.mask_prev[row], &mut self.mask_cur[row]);
                }
            }
            self.wraps_prev.copy_from_slice(&self.wraps);
        }

        // Phase 3: rebuild.
        for r in 0..rows {
            if !self.dirty[r] {
                continue;
            }
            build_row_plan(
                frame.row(r),
                ctx,
                &self.mask_prev[r],
                scratch,
                glyphs,
                stats,
                &mut self.rows[r],
            );
            stats.rows_planned += 1;
        }

        for r in 0..rows {
            self.keys[r] = frame.row_key(r);
        }
        self.grid = Some(size);
        self.style = Some(style_key);
        self.cell = Some(cell);
    }

    /// Fill `self.scan` with the rows a URL rescan must cover: the dirty rows,
    /// plus display row 0 when the viewport scrolled, all closed under wrap runs
    /// (`US-0092`).
    ///
    /// Rows `r` and `r + 1` count as connected when **either** frame's flags say
    /// so: a row that has just lost its `WRAPLINE` is dirty, and its old
    /// continuation row — untouched, so not dirty — still carries a mask that
    /// was extended from it and must be recomputed.
    ///
    /// `scrolled_seam` covers the one dependency the run closure cannot express:
    /// row 0's mask also depends on whether its wrap-connected predecessor is
    /// still on screen, and a scroll changes that with no row key changing.
    /// There is no `connected(-1)` to walk, so the seam is seeded directly.
    fn mark_scan_runs(&mut self, rows: usize, scrolled_seam: bool) {
        let Self {
            dirty,
            wraps,
            wraps_prev,
            scan,
            ..
        } = self;
        scan.clear();
        scan.resize(rows, false);
        let connected = |i: usize| wraps[i] || wraps_prev[i];
        let seed = |r: usize| dirty[r] || (scrolled_seam && r == 0);
        let mut r = 0;
        while r < rows {
            if !seed(r) {
                r += 1;
                continue;
            }
            let mut start = r;
            while start > 0 && connected(start - 1) {
                start -= 1;
            }
            let mut end = r;
            while end + 1 < rows && connected(end) {
                end += 1;
            }
            scan[start..=end].fill(true);
            r = end + 1;
        }
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
        // The masks and the wrap flags they were computed from travel with
        // their rows too, so the delta check and the wrap-run walk stay
        // meaningful and only the scrolled-in rows are rescanned.
        if self.mask_prev.len() == len {
            if scrolled > 0 {
                self.mask_prev.rotate_left(distance);
            } else {
                self.mask_prev.rotate_right(distance);
            }
        }
        if self.wraps_prev.len() == len {
            if scrolled > 0 {
                self.wraps_prev.rotate_left(distance);
            } else {
                self.wraps_prev.rotate_right(distance);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_terminal::test_support::FixtureCell;

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

    /// `US-0092`: the URL pass costs the rows that changed, not the viewport.
    /// One rewritten row in a 45x160 viewport with no wraps scans one row.
    #[gpui::test]
    fn url_pass_scans_the_changed_rows_not_the_viewport(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(45);
        let (mut frame, mut fixture) = frame_with(&texts, 160).build_with_fixture();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.url_scans, 1);
        assert_eq!(
            s.url_rows_scanned, 45,
            "nothing is cached on the first frame"
        );

        rewrite_row(&mut frame, &mut fixture, 20, "echoed input");
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.url_scans, 1, "a scan still happens exactly when it did");
        assert_eq!(s.url_rows_scanned, 1, "and it walks one row, not 45");

        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.url_scans, 0, "an idle frame still scans nothing");
        assert_eq!(s.url_rows_scanned, 0);
    }

    /// `US-0092`: a dirty-rows-only scope would mis-underline this. A URL
    /// wrapping over three rows where only the middle row is rewritten must
    /// produce the mask a whole-viewport rescan produces.
    #[gpui::test]
    fn url_pass_rescans_the_whole_wrap_run_of_a_changed_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 10)
            .text(0, 0, "https://a.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "test/bbbbb")
            .flags(1, 9, CellFlags::WRAPLINE)
            .text(2, 0, "ccc rest")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));

        // Rewrite the middle row, keeping its wrap: `write` clears the flag, so
        // the fixture re-sets it exactly as the engine's print path does.
        fixture.begin_batch();
        for (col, ch) in "test/ddddd".chars().enumerate() {
            fixture.write(
                1,
                col,
                &FixtureCell {
                    ch,
                    ..FixtureCell::default()
                },
            );
        }
        fixture.set_wrapped(1, true);
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(
            s.url_rows_scanned, 3,
            "the changed row pulls in the rest of its wrap run, and nothing else"
        );

        let mut want = Vec::new();
        let mut wraps = Vec::new();
        crate::url::url_masks_into(&frame, &mut want, &mut wraps);
        let got: Vec<Vec<bool>> = (0..5).map(|r| h.cache.url_mask(r).to_vec()).collect();
        assert_eq!(got, want, "same mask as a full rescan");
        assert!(
            got[0].iter().all(|&m| m) && got[1].iter().all(|&m| m),
            "the URL still covers both wrapped rows: {got:?}"
        );
        assert_eq!(&got[2][..4], &[true, true, true, false], "{:?}", got[2]);
    }

    /// A row that loses its `WRAPLINE` is dirty, but the continuation row that
    /// inherited its mask is not — so the run walk has to use the union of both
    /// frames' wrap flags, or the stale underline survives (`US-0092`).
    #[gpui::test]
    fn url_pass_rescans_a_continuation_row_whose_wrap_was_dropped(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(3, 10)
            .text(0, 0, "https://a.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "test/bbbbb")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));
        assert!(h.cache.url_mask(1)[0], "row 1 starts as a continuation");

        // Row 0 stops being a URL and stops wrapping (a write clears the flag);
        // row 1 is untouched, so only the union of both frames' wrap flags
        // brings it back into the rescan.
        rewrite_row(&mut frame, &mut fixture, 0, "plain text");
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(
            s.url_rows_scanned, 2,
            "the old continuation row is rescanned"
        );
        assert!(
            h.cache.url_mask(1).iter().all(|&m| !m),
            "the stale underline is gone: {:?}",
            h.cache.url_mask(1)
        );
    }

    // ───────────────── US-0092 independent verification ─────────────────

    /// The oracle: what a whole-viewport rescan of this very frame produces.
    /// Every incremental scan must agree with it, row for row.
    #[track_caller]
    fn assert_masks_match_a_full_rescan(cache: &PlanCache, frame: &Frame, what: &str) {
        let rows = usize::from(frame.size().rows);
        let mut want = Vec::new();
        let mut wraps = Vec::new();
        crate::url::url_masks_into(frame, &mut want, &mut wraps);
        let got: Vec<Vec<bool>> = (0..rows).map(|r| cache.url_mask(r).to_vec()).collect();
        for r in 0..rows {
            assert_eq!(
                got[r], want[r],
                "{what}: row {r} mask differs from a full rescan\n got {:?}\nwant {:?}",
                got[r], want[r]
            );
        }
    }

    /// A three-row wrapped URL where only the FIRST row is rewritten.
    #[gpui::test]
    fn url_v2_first_row_of_a_three_row_url_changes(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 10)
            .text(0, 0, "https://a.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "test/bbbbb")
            .flags(1, 9, CellFlags::WRAPLINE)
            .text(2, 0, "ccc rest")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));

        fixture.begin_batch();
        for (col, ch) in "https://z.".chars().enumerate() {
            fixture.write(
                0,
                col,
                &FixtureCell {
                    ch,
                    ..FixtureCell::default()
                },
            );
        }
        fixture.set_wrapped(0, true);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "first row rewritten");
    }

    /// ...and where only the LAST row is rewritten, so the URL's tail stops
    /// being part of it.
    #[gpui::test]
    fn url_v2_last_row_of_a_three_row_url_changes(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 10)
            .text(0, 0, "https://a.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "test/bbbbb")
            .flags(1, 9, CellFlags::WRAPLINE)
            .text(2, 0, "ccc rest")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));

        rewrite_row(&mut frame, &mut fixture, 2, " plain    ");
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "last row rewritten");
    }

    /// The viewport scrolled a row at a time and back. An index-keyed URL cache
    /// would keep the mask of whatever row used to sit at that index.
    #[gpui::test]
    fn url_v2_scrolling_the_viewport_keeps_the_masks_exact(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 24).build_with_fixture();
        fixture.feed(
            b"top line\r\nhttps://wrapped.test/aaaaaaaaaaaaaaaaaaaaaa\r\nplain\r\nmore\r\nlast\r\ntail\r\ntail2",
        );
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "before scrolling");

        for back in 1..=3 {
            fixture.scroll_back(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("scrolled back {back}"));
        }
        for fwd in 1..=3 {
            fixture.scroll_forward(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("scrolled forward {fwd}"));
        }
    }

    /// `DL` deletes the middle row of a wrapped URL; `IL` pushes a continuation
    /// row down. Both change wrap connectivity.
    #[gpui::test]
    fn url_v2_delete_and_insert_line_inside_a_wrapped_url(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 16).build_with_fixture();
        fixture.feed(b"https://wrapped.test/aaaaaaaaaaaa\r\ntail");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "wrapped url laid out");

        fixture.feed(b"\x1b[2;1H\x1b[M");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "after DL");

        fixture.feed(b"\x1b[2;1H\x1b[L");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "after IL");
    }

    /// `CSI 2 J`, the alternate screen and back.
    #[gpui::test]
    fn url_v2_clear_screen_and_alt_screen_swap(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 20).build_with_fixture();
        fixture.feed(b"see https://a.test/x\r\nand https://b.test/y");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "urls on screen");

        fixture.feed(b"\x1b[?1049h");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "alt screen entered");

        fixture.feed(b"alt https://c.test/z");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "alt screen url");

        fixture.feed(b"\x1b[2J");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "after CSI 2 J");

        fixture.feed(b"\x1b[?1049l");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "back on the primary screen");
    }

    /// A resize re-wraps the URL over a different number of rows.
    #[gpui::test]
    fn url_v2_resize_rewraps_the_url(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 30).build_with_fixture();
        fixture.feed(b"go https://wrapped.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaa end");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "at 30 columns");

        for cols in [18u16, 12, 40] {
            fixture.terminal().resize(
                oneterm_terminal::Size { rows: 6, cols },
                oneterm_terminal::ResizePolicy::Default.into(),
            );
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("at {cols} columns"));
        }
    }

    /// Scrollback churn: many lines pushed through, URLs among them, and the
    /// masks checked on every frame.
    #[gpui::test]
    fn url_v2_streaming_output_keeps_the_masks_exact(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 24).build_with_fixture();
        for i in 0..40 {
            if i % 7 == 0 {
                fixture.feed(b"https://stream.test/aaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
            } else {
                fixture.feed(format!("line {i}\r\n").as_bytes());
            }
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("after line {i}"));
        }
    }

    /// Two feeds with no render between them (a skipped frame), the first of
    /// which drops a wrap flag. `wraps_prev` is rotated per *update*, not per
    /// feed, so the union still has to reach the old continuation row.
    #[gpui::test]
    fn url_v2_wrap_dropped_in_a_frame_that_was_never_rendered(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(4, 10)
            .text(0, 0, "https://a.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "test/bbbbb")
            .build_with_fixture();
        h.update(cx, &frame, style_key(13.0));
        assert!(h.cache.url_mask(1)[0], "row 1 starts as a continuation");

        // Feed 1: row 0 stops being a URL and stops wrapping. No render.
        fixture.begin_batch();
        for (col, ch) in "plain text".chars().enumerate() {
            fixture.write(
                0,
                col,
                &FixtureCell {
                    ch,
                    ..FixtureCell::default()
                },
            );
        }
        // Feed 2: an unrelated row changes. Only now is a frame rendered.
        fixture.begin_batch();
        for (col, ch) in "zzz".chars().enumerate() {
            fixture.write(
                3,
                col,
                &FixtureCell {
                    ch,
                    ..FixtureCell::default()
                },
            );
        }
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "wrap dropped in a skipped frame");
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
