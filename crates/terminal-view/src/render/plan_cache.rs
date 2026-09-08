//! Per-row plan cache: decides which display rows must be rebuilt this frame
//! (damage ∪ cursor row ∪ URL-mask delta ∪ scrolled-in rows), hash-verifies
//! the candidates and rebuilds only the rows whose content changed.

use super::diagnostics::FrameStats;
use super::frame::{Damage, Frame, GridSize};
use super::glyphs::GlyphCache;
use super::row_plan::{PlanContext, RowPlan, Scratch, build_row_plan};
use crate::url::url_masks_into;

/// Everything besides cell content that changes how a row is planned. A
/// change zeroes every plan hash so rows rebuild without hashing.
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
    candidate: Vec<bool>,
    dirty: Vec<bool>,
    style: Option<StyleKey>,
    grid: Option<GridSize>,
    display_offset: usize,
    mask_prev: Vec<Vec<bool>>,
    mask_cur: Vec<Vec<bool>>,
    wraps: Vec<bool>,
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
            candidate: Vec::new(),
            dirty: Vec::new(),
            style: None,
            grid: None,
            display_offset: 0,
            mask_prev: Vec::new(),
            mask_cur: Vec::new(),
            wraps: Vec::new(),
        }
    }

    pub(crate) fn rows(&self) -> &[RowPlan] {
        &self.rows
    }

    pub(crate) fn row(&self, r: usize) -> Option<&RowPlan> {
        self.rows.get(r)
    }

    /// The URL mask of display row `r` as of the last update (empty when the
    /// row had no URL or was never scanned).
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
        let full = self.grid != Some(size) || self.style != Some(style_key);
        if full {
            self.rows.resize_with(rows, RowPlan::default);
            for plan in &mut self.rows {
                plan.invalidate();
            }
        } else {
            self.rotate(frame.display_offset(), rows);
        }

        // Phase 1: candidates = damage ∪ cursor row ∪ never-built rows.
        self.candidate.clear();
        self.candidate.resize(rows, false);
        match frame.damage() {
            Damage::Full => self.candidate.fill(true),
            Damage::Rows(list) => {
                for &r in list {
                    // A row beyond the grid is a race with a resize; ignore it.
                    if let Some(c) = self.candidate.get_mut(r) {
                        *c = true;
                    }
                }
            }
        }
        let cursor = frame.cursor();
        if cursor.row >= 0
            && let Some(c) = self.candidate.get_mut(cursor.row as usize)
        {
            *c = true;
        }
        for (c, plan) in self.candidate.iter_mut().zip(&self.rows) {
            if plan.hash == 0 {
                *c = true;
            }
        }
        stats.rows_candidate = self.candidate.iter().filter(|&&c| c).count() as u32;

        // Phase 2: hash-verify; `dirty` keeps only the rows that really changed.
        self.dirty.clear();
        self.dirty.resize(rows, false);
        for r in 0..rows {
            if self.candidate[r] && frame.row(r).hash() != self.rows[r].hash {
                self.dirty[r] = true;
            }
        }

        // Phase 3: a changed row may start or end a wrapped URL, which changes
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

        // Phase 4: rebuild.
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
        self.grid = Some(size);
        self.style = Some(style_key);
        self.display_offset = frame.display_offset();
        stats.rows_total = rows as u32;
    }

    /// Scrolling moves plans with their rows; only the scrolled-in rows lose
    /// their plan (deviation 8).
    fn rotate(&mut self, display_offset: usize, rows: usize) {
        let delta = display_offset as i64 - self.display_offset as i64;
        if delta == 0 {
            return;
        }
        if delta.unsigned_abs() as usize >= rows {
            for plan in &mut self.rows {
                plan.invalidate();
            }
            return;
        }
        if delta > 0 {
            // Scrolled into history: content moves down.
            let d = delta as usize;
            self.rows.rotate_right(d);
            for plan in &mut self.rows[..d] {
                plan.invalidate();
            }
        } else {
            let d = (-delta) as usize;
            self.rows.rotate_left(d);
            for plan in &mut self.rows[rows - d..] {
                plan.invalidate();
            }
        }
        // Masks are recomputed whenever any row is dirty, which scrolling
        // guarantees; rotating them keeps the delta check meaningful.
        if self.mask_prev.len() == rows {
            if delta > 0 {
                self.mask_prev.rotate_right(delta as usize);
            } else {
                self.mask_prev.rotate_left((-delta) as usize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, TestAppContext, VisualTestContext, px};

    use super::*;
    use crate::render::frame::test_support::FrameBuilder;
    use crate::render::frame::{CellFlags, CursorShape, GridPoint};
    use crate::render::glyphs::FontSet;
    use crate::render::shapes::CellSizeDevicePx;
    use crate::theme::{TerminalTheme, build_terminal_theme};

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
                    cell_width: px(8.0),
                    device: CellSizeDevicePx { w: 8, h: 16 },
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

    #[gpui::test]
    fn plan_cache_first_frame_plans_every_row_and_idle_plans_none(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(10);
        let frame = frame_with(&texts, 20).build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 10);
        assert_eq!(s.rows_candidate, 10);
        assert_eq!(s.url_scans, 1);
        let idle = frame_with(&texts, 20).damage_rows(&[]).build();
        let s = h.update(cx, &idle, style_key(13.0));
        assert_eq!(s.rows_planned, 0);
        assert_eq!(s.rows_candidate, 1, "the cursor row is always a candidate");
        assert_eq!(s.url_scans, 0);
        assert_eq!(s.shape_calls, 0);
    }

    #[gpui::test]
    fn scroll_rotates_plans_and_replans_only_scrolled_in_rows(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(10);
        let frame = frame_with(&texts, 20).build();
        h.update(cx, &frame, style_key(13.0));
        let hash_row0 = h.cache.rows[0].hash;

        // Scroll 3 rows into history: three new rows on top, everything else
        // moves down, and the engine reports `Damage::Full`.
        let mut scrolled: Vec<String> = (0..3).map(|i| format!("history {i}")).collect();
        scrolled.extend(texts[..7].iter().cloned());
        let frame = frame_with(&scrolled, 20).display_offset(3).build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(
            s.rows_candidate, 10,
            "Damage::Full makes every row a candidate"
        );
        assert_eq!(s.rows_planned, 3, "only the scrolled-in rows rebuild");
        assert_eq!(
            h.cache.rows[3].hash, hash_row0,
            "plans moved with their rows"
        );

        // Scroll back down by 2.
        let mut back: Vec<String> = scrolled[2..].to_vec();
        back.push("new bottom 0".into());
        back.push("new bottom 1".into());
        let frame = frame_with(&back, 20).display_offset(1).build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 2);

        // A jump of a whole screen rebuilds everything.
        let frame = frame_with(&lines(10), 20).display_offset(30).build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 10);
    }

    #[gpui::test]
    fn cursor_row_replans_on_undamaged_change(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        let frame = frame_with(&texts, 20).build();
        h.update(cx, &frame, style_key(13.0));
        let mut changed = texts.clone();
        changed[2] = "echoed input".into();
        let frame = frame_with(&changed, 20)
            .damage_rows(&[])
            .cursor(2, 5, CursorShape::Block)
            .build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_candidate, 1);
        assert_eq!(s.rows_planned, 1);
        // A damage list that names a row past the grid is ignored.
        let frame = frame_with(&changed, 20).damage_rows(&[99]).build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 0);
    }

    #[gpui::test]
    fn selection_change_does_not_replan(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        h.update(cx, &frame_with(&texts, 20).build(), style_key(13.0));
        let frame = frame_with(&texts, 20)
            .selection(
                GridPoint { row: 1, col: 0 },
                GridPoint { row: 2, col: 4 },
                false,
            )
            .build();
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.rows_planned, 0, "Damage::Full is hash-verified");
        assert_eq!(s.rows_candidate, 5);
        assert_eq!(s.url_scans, 0, "no changed row, no URL scan");
    }

    #[gpui::test]
    fn style_key_change_replans_all(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let texts = lines(5);
        h.update(cx, &frame_with(&texts, 20).build(), style_key(13.0));
        let frame = frame_with(&texts, 20).damage_rows(&[]).build();
        let s = h.update(cx, &frame, style_key(14.0));
        assert_eq!(s.rows_planned, 5);
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

    #[gpui::test]
    fn url_mask_delta_replans_continuation_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let base = FrameBuilder::new(3, 10)
            .text(0, 0, "https://x.")
            .text(1, 0, "com/path")
            .build();
        h.update(cx, &base, style_key(13.0));
        assert!(h.cache.url_mask(1).is_empty() || !h.cache.url_mask(1)[0]);
        // Only row 0 is damaged (it now wraps), but row 1 becomes a URL
        // continuation and must replan too.
        let wrapped = FrameBuilder::new(3, 10)
            .text(0, 0, "https://x.")
            .flags(0, 9, CellFlags::WRAPLINE)
            .text(1, 0, "com/path")
            .damage_rows(&[0])
            .build();
        let s = h.update(cx, &wrapped, style_key(13.0));
        assert_eq!(s.rows_planned, 2);
        assert!(h.cache.url_mask(1)[0]);
        assert_eq!(h.cache.row(1).map(|p| p.decorations.len()), Some(1));
    }
}
