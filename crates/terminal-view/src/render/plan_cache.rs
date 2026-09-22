//! Per-row plan cache, keyed on row identity.
//!
//! `US-0085` replaced the old three-phase selection — damage ∪ cursor row ∪
//! never-built rows, then a per-row content hash to verify the candidates —
//! with one comparison. A `RowKey` is `(RowId, SeqNo)`: the engine stamps a row
//! only when it actually changes, and copies it into this view's snapshot state
//! only when that stamp passed this view's watermark, so
//! `stored_key != frame.row_key(r)` **is** the answer. The hash was there
//! because the old damage escalated to `Full` on every scroll; the render
//! state reports a scroll as a delta instead, so the plans shift with their rows
//! and only the rows that really changed are rebuilt.
//!
//! An `Unchanged` frame returns before any of that: no key scan, no URL scan, no
//! layout.

use oneterm_highlight::RowRoles;
use oneterm_terminal::SnapshotUpdate;

use super::diagnostics::FrameStats;
use super::frame::{Frame, GridSize, RowKey};
use super::glyphs::GlyphCache;
use super::row_plan::{LineMark, PlanContext, RowPlan, Scratch, build_row_plan, class_rows_into};
use super::shapes::CellSizeDevicePx;
use crate::highlight::SemanticOverlay;
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
    /// `DECSCNM` (`? 5`). Part of the key because it changes what the two
    /// default colours resolve to for every cell, so a plan built under it is
    /// not reusable once it clears.
    pub reverse_video: bool,
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
    /// The semantic classes as of the last update, and this frame's scratch —
    /// the same pair as the URL masks, because a class depends on the whole
    /// logical line and therefore on the neighbouring rows of a wrap run
    /// (`BUG-0071`). Empty per row while semantic highlighting is off.
    class_prev: Vec<Vec<u8>>,
    class_cur: Vec<Vec<u8>>,
    /// The OSC 133 role of each display row's logical line, as of the last
    /// update — authoritative for every row for the same reason the masks and
    /// the classes are: a row outside this frame's rescan did not change, so
    /// neither did its role (`US-0133`).
    roles: RowRoles,
    roles_cur: RowRoles,
    /// What the OSC 133 marks say about each display row's logical line before
    /// the transition rule: the region it starts in, and whether it closed its
    /// prompt. Authoritative for every row like `roles`, because a line asks its
    /// predecessor and the predecessor is often outside this frame's rescan
    /// (`US-0133` rework, MAJ-1 and RV-MAJ-1).
    line_marks: Vec<LineMark>,
    /// The prompt run the exit code tints, and the code, as of the last update.
    /// A change dirties both the run it leaves and the run it lands on.
    tint: Option<(std::ops::Range<usize>, i32)>,
    /// The exit code the last update saw, so a frame that changes nothing else
    /// is not taken as "nothing to do".
    exit_code: Option<i32>,
    /// This frame's per-row `WRAPLINE` flags.
    wraps: Vec<bool>,
    /// The previous frame's, aligned with `mask_prev` (so `shift` rotates it
    /// too). A row whose wrap flag was *dropped* still has to pull its old
    /// continuation row into the rescan, so the run walk uses the union.
    wraps_prev: Vec<bool>,
    /// The rows this frame rescans for URL masks: the dirty rows closed under
    /// wrap runs (`US-0092`).
    scan: Vec<bool>,
    /// The rows this frame rescans for semantic classes: `scan`, plus the one
    /// logical line after each changed run, because a line's OSC 133 role is
    /// read from the region its predecessor started in (`US-0133`).
    scan_class: Vec<bool>,
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
            class_prev: Vec::new(),
            class_cur: Vec::new(),
            roles: RowRoles::default(),
            roles_cur: RowRoles::default(),
            line_marks: Vec::new(),
            tint: None,
            exit_code: None,
            wraps: Vec::new(),
            wraps_prev: Vec::new(),
            scan: Vec::new(),
            scan_class: Vec::new(),
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

    /// The semantic classes of display row `r` as of the last update (empty
    /// while semantic highlighting is off).
    #[cfg(test)]
    pub(crate) fn classes(&self, r: usize) -> &[u8] {
        self.class_prev.get(r).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The OSC 133 role of display row `r` as of the last update.
    #[cfg(test)]
    pub(crate) fn role(&self, r: usize) -> Option<oneterm_highlight::RowRole> {
        self.roles.role_at(r)
    }

    /// The prompt run the exit code tints, and the code.
    #[cfg(test)]
    pub(crate) fn tint(&self) -> Option<(std::ops::Range<usize>, i32)> {
        self.tint.clone()
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

        // The exit code is the one input that is not in the frame, so an
        // `Unchanged` frame can still carry a new one (`OSC 133;D` arriving with
        // no redraw). Taking the early return on it would drop the tint until
        // something else dirtied a row.
        let exit_code = ctx.semantic.and_then(SemanticOverlay::exit_code);
        stats.rows_total = rows as u32;
        if !restyled && frame.update() == SnapshotUpdate::Unchanged && exit_code == self.exit_code {
            // Nothing moved and nothing was copied: the plans, the masks and
            // the keys all still describe this frame.
            stats.frames_unchanged += 1;
            return;
        }
        self.exit_code = exit_code;

        if restyled {
            self.rows.resize_with(rows, RowPlan::default);
            self.keys.clear();
            self.keys.resize(rows, None);
        } else {
            if let SnapshotUpdate::Partial { scrolled } = frame.update() {
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
        // the class of untouched continuation rows (deviation 10). The semantic
        // classes have the same dependency and a stronger one: a logical line is
        // scanned as a whole, so every row of a wrap run depends on every other
        // (`BUG-0071`). That hazard is bounded, and the bound is the scope of
        // the rescan: neither a URL nor a scanner state can reach a row it is
        // not wrap-connected to, so the answer is the dirty rows closed under
        // the wrap runs `self.wraps` already tracks — never the whole viewport
        // (`US-0092`). Rows outside those runs keep the masks and classes they
        // had, which is why `mask_prev` and `class_prev` stay authoritative for
        // all rows.
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
            matches!(frame.update(), SnapshotUpdate::Partial { scrolled } if scrolled != 0);
        let any_dirty = self.dirty.iter().any(|&d| d);
        if any_dirty {
            fill_wraps(frame, &mut self.wraps);
            self.wraps_prev.resize(rows, false);
            self.mask_prev.resize_with(rows, Vec::new);
            self.mask_cur.resize_with(rows, Vec::new);
            self.class_prev.resize_with(rows, Vec::new);
            self.class_cur.resize_with(rows, Vec::new);
            self.roles.role.resize(rows, None);
            self.roles_cur.role.resize(rows, None);
            self.line_marks.resize(rows, LineMark::default());
            self.mark_scan_runs(rows, scrolled_seam);
            stats.url_scans += 1;

            // The URL masks, over the dirty rows closed under wrap runs — the
            // `US-0092` bound, unchanged.
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
            }

            // The semantic classes, over the same runs plus one logical line
            // forward, because a line's OSC 133 role is read from the region its
            // predecessor started in (`US-0133`). Only this pass pays for that.
            let mut r = 0;
            while r < rows {
                if !self.scan_class[r] {
                    r += 1;
                    continue;
                }
                let start = r;
                while r < rows && self.scan_class[r] {
                    r += 1;
                }
                match ctx.semantic {
                    Some(overlay) => class_rows_into(
                        frame,
                        overlay,
                        &mut self.class_cur,
                        &mut self.roles_cur.role,
                        &mut self.line_marks,
                        &self.wraps,
                        start..r,
                        scratch,
                        stats,
                    ),
                    None => {
                        self.class_cur[start..r].iter_mut().for_each(Vec::clear);
                        self.roles_cur.role[start..r].fill(None);
                        self.line_marks[start..r].fill(LineMark::default());
                    }
                }
            }

            // One delta per row, over whichever of the two a row was in: a row
            // the other pass skipped still holds last frame's answer there, and
            // comparing that against a stale scratch buffer would corrupt it.
            for row in 0..rows {
                let (url, class) = (self.scan[row], self.scan_class[row]);
                let mut changed = false;
                if url {
                    changed |= self.mask_cur[row] != self.mask_prev[row];
                    std::mem::swap(&mut self.mask_prev[row], &mut self.mask_cur[row]);
                }
                if class {
                    changed |= self.class_cur[row] != self.class_prev[row]
                        || self.roles_cur.role[row] != self.roles.role[row];
                    std::mem::swap(&mut self.class_prev[row], &mut self.class_cur[row]);
                    self.roles.role[row] = self.roles_cur.role[row];
                }
                if changed {
                    self.dirty[row] = true;
                }
            }
            self.wraps_prev.copy_from_slice(&self.wraps);
        }

        // Phase 2b: the exit-code tint. `self.roles` is authoritative for the
        // whole viewport once the rescan is in, so the prompt the most recently
        // completed block belongs to is decided here rather than inside the
        // scan — where it would depend on rows the scan deliberately skipped.
        self.update_tint(exit_code, rows);

        // Phase 3: rebuild.
        for r in 0..rows {
            if !self.dirty[r] {
                continue;
            }
            let tint = self
                .tint
                .as_ref()
                .filter(|(run, _)| run.contains(&r))
                .map(|(_, code)| *code);
            build_row_plan(
                frame.row(r),
                ctx,
                &self.class_prev[r],
                &self.mask_prev[r],
                self.roles.role_at(r),
                tint,
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

    /// Point the exit-code tint at the prompt of the most recently completed
    /// command block, and dirty the rows it moved off and on to.
    ///
    /// No rescan is needed for either: the tint is a class substitution the row
    /// plan applies over classes that are already correct, so a plan rebuild is
    /// the whole of the invalidation.
    fn update_tint(&mut self, exit_code: Option<i32>, rows: usize) {
        let next = match exit_code.filter(|_| self.roles.role.len() == rows) {
            Some(code) => self
                .roles
                .last_completed_prompt(&self.wraps)
                .map(|run| (run, code)),
            None => None,
        };
        if next == self.tint {
            return;
        }
        for run in [self.tint.as_ref(), next.as_ref()].into_iter().flatten() {
            for r in run.0.clone() {
                if let Some(dirty) = self.dirty.get_mut(r) {
                    *dirty = true;
                }
            }
        }
        self.tint = next;
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
    ///
    /// One further dependency reaches **forward** by exactly one logical line:
    /// a line's OSC 133 role is decided from the region its predecessor started
    /// in (`US-0133`'s transition rule), so a run whose own content changed may
    /// change the role of the line below it. That line is therefore pulled into
    /// the rescan too — and only that one, because a line reached this way did
    /// not change, so its own region did not either and the chain stops.
    fn mark_scan_runs(&mut self, rows: usize, scrolled_seam: bool) {
        let Self {
            dirty,
            wraps,
            wraps_prev,
            scan,
            scan_class,
            ..
        } = self;
        scan.clear();
        scan.resize(rows, false);
        scan_class.clear();
        scan_class.resize(rows, false);
        let connected = |i: usize| wraps[i] || wraps_prev[i];
        let seed = |r: usize| dirty[r] || (scrolled_seam && r == 0);
        let mut r = 0;
        let mut carry = false;
        while r < rows {
            let seeded = seed(r);
            if !seeded && !carry {
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
            let changed = (start..=end).any(seed);
            if changed {
                scan[start..=end].fill(true);
            }
            scan_class[start..=end].fill(true);
            // Only a run that actually changed can move the line below it, so
            // the chain is one line long and never cascades.
            carry = changed;
            r = end + 1;
        }
    }

    /// Scrolling moves plans with their rows, exactly as the snapshot state moves
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
        // The masks, the classes and the wrap flags they were computed from
        // travel with their rows too, so the delta check and the wrap-run walk
        // stay meaningful and only the scrolled-in rows are rescanned.
        if self.mask_prev.len() == len {
            if scrolled > 0 {
                self.mask_prev.rotate_left(distance);
            } else {
                self.mask_prev.rotate_right(distance);
            }
        }
        if self.class_prev.len() == len {
            if scrolled > 0 {
                self.class_prev.rotate_left(distance);
            } else {
                self.class_prev.rotate_right(distance);
            }
        }
        if self.roles.role.len() == len {
            if scrolled > 0 {
                self.roles.role.rotate_left(distance);
            } else {
                self.roles.role.rotate_right(distance);
            }
        }
        if self.line_marks.len() == len {
            if scrolled > 0 {
                self.line_marks.rotate_left(distance);
            } else {
                self.line_marks.rotate_right(distance);
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
    use oneterm_highlight::RowRole;
    use oneterm_terminal::Semantic;
    use oneterm_terminal::test_support::FixtureCell;

    use crate::highlight::SemanticOverlay;
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
            reverse_video: false,
        }
    }

    struct Harness {
        theme: TerminalTheme,
        fonts: FontSet,
        cache: PlanCache,
        scratch: Scratch,
        glyphs: GlyphCache,
        semantic: Option<SemanticOverlay>,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                theme: build_terminal_theme(&gpui_component::Theme::default()),
                fonts: FontSet::new(&font(), px(13.0)),
                cache: PlanCache::new(),
                scratch: Scratch::new(),
                glyphs: GlyphCache::new(),
                semantic: None,
            }
        }

        /// The same harness with semantic highlighting on (`BUG-0071`).
        fn semantic() -> Self {
            Self {
                semantic: Some(SemanticOverlay::new(
                    oneterm_highlight::ShellProfile::Unix,
                    true,
                )),
                ..Self::new()
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
                semantic,
            } = self;
            cx.update(|window, _| {
                let ctx = PlanContext {
                    theme,
                    fonts,
                    font_size: px(13.0),
                    font_weight: 400.0,
                    cell_width: px(cell_width),
                    device,
                    semantic: semantic.as_ref(),
                    reverse_video: false,
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
        assert_eq!(frame.update(), SnapshotUpdate::Partial { scrolled: -2 });
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
            SnapshotUpdate::Full,
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
                oneterm_terminal::ResizePolicy::BottomAnchor,
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

    /// `US-0092` re-verification: 200 randomized scroll / feed / resize steps,
    /// every frame compared against a from-scratch `url_masks_into` of that same
    /// frame. Deterministic (a fixed LCG), so a failure is reproducible.
    ///
    /// This is the shape that found defect 2: the bug needed a *forward* scroll
    /// past the head of a wrap run, which no hand-written case had reached.
    #[gpui::test]
    fn url_v2_randomized_scroll_and_feed_matches_a_full_rescan(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(8, 28).build_with_fixture();

        // xorshift, so the sequence is fixed and the failure reproducible.
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for step in 0..200 {
            match next() % 8 {
                // A URL long enough to wrap over two or three rows.
                0 | 1 => {
                    fixture.feed(b"go https://wrapped.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa end\r\n")
                }
                2 => fixture.feed(b"short https://x.test/p\r\n"),
                3 => fixture.feed(format!("plain line {step}\r\n").as_bytes()),
                // Scroll back: rows arrive at the top, so they are dirty.
                4 => fixture.scroll_back((next() % 5) as usize + 1),
                // Scroll forward: rows leave the top without changing, which is
                // where the viewport seam matters.
                5 => fixture.scroll_forward((next() % 5) as usize + 1),
                6 => fixture.scroll_forward(1),
                _ => {
                    let cols = [16u16, 22, 28, 34][(next() % 4) as usize];
                    fixture.terminal().resize(
                        oneterm_terminal::Size { rows: 8, cols },
                        oneterm_terminal::ResizePolicy::BottomAnchor,
                    );
                }
            }
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("step {step}"));
        }
    }

    /// The seam seed costs one row and no replan when display row 0 is not part
    /// of a wrap run and its mask did not change (`US-0092` rework).
    #[gpui::test]
    fn url_v2_scrolled_seam_costs_one_row_when_row_zero_does_not_wrap(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 30).build_with_fixture();
        fixture.feed(b"aaa\r\nbbb\r\nccc\r\nddd\r\neee\r\nfff\r\nggg\r\nhhh\r\niii\r\njjj");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        fixture.scroll_back(3);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        // Forward one row: nothing wraps, so the seam is exactly one row and its
        // mask is unchanged, so no plan is rebuilt for it.
        fixture.scroll_forward(1);
        resnapshot(&mut frame, &mut fixture);
        let s = h.update(cx, &frame, style_key(13.0));
        assert_eq!(s.url_rows_scanned, 2, "one scrolled-in row plus the seam");
        assert_eq!(s.rows_planned, 1, "only the scrolled-in row replans");
        assert_masks_match_a_full_rescan(&h.cache, &frame, "seam with no wrap");
    }

    /// A scroll that lands the viewport top exactly on the head row of a wrap
    /// run: the head's own prefix is on screen, so the mask is the full-rescan
    /// mask and the run below it is rescanned with it (`US-0092` rework).
    #[gpui::test]
    fn url_v2_scroll_landing_on_the_head_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 24).build_with_fixture();
        fixture.feed(b"pre\r\nhttps://wrapped.test/aaaaaaaaaaaaaaaaaaaaaaaa\r\npost\r\none\r\ntwo\r\nthree\r\nfour");
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        fixture.scroll_back(5);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        for step in 0..6 {
            fixture.scroll_forward(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(&h.cache, &frame, &format!("forward step {step}"));
        }
    }

    /// A scroll larger than the viewport: the cache cannot shift, so every key
    /// is dropped and every row rescanned — the seam is moot but must not break
    /// the result (`US-0092` rework).
    #[gpui::test]
    fn url_v2_scroll_further_than_the_viewport(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 24).build_with_fixture();
        for i in 0..20 {
            if i % 4 == 0 {
                fixture.feed(b"https://wrapped.test/aaaaaaaaaaaaaaaaaaaaaaaa\r\n");
            } else {
                fixture.feed(format!("line {i}\r\n").as_bytes());
            }
        }
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        fixture.scroll_back(12);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "jumped back 12 rows");

        fixture.scroll_forward(12);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));
        assert_masks_match_a_full_rescan(&h.cache, &frame, "jumped forward 12 rows");
    }

    /// A `Full` update (a resize) taken while the viewport is already scrolled
    /// back: every key is dropped, so row 0 is rescanned by virtue of being
    /// dirty and the seam seed is not what saves it (`US-0092` rework).
    #[gpui::test]
    fn url_v2_resize_while_scrolled_back(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::new();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 30).build_with_fixture();
        for i in 0..20 {
            if i % 3 == 0 {
                fixture.feed(b"https://wrapped.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n");
            } else {
                fixture.feed(format!("line {i}\r\n").as_bytes());
            }
        }
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        fixture.scroll_back(7);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, style_key(13.0));

        for cols in [18u16, 40, 24] {
            fixture.terminal().resize(
                oneterm_terminal::Size { rows: 6, cols },
                oneterm_terminal::ResizePolicy::BottomAnchor,
            );
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, style_key(13.0));
            assert_masks_match_a_full_rescan(
                &h.cache,
                &frame,
                &format!("resized to {cols} while scrolled back"),
            );
        }
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

    fn semantic_key() -> StyleKey {
        let mut key = style_key(13.0);
        key.semantic_enabled = true;
        key
    }

    /// `BUG-0071`: a class depends on the whole logical line, so rewriting the
    /// head row of a wrapped line must replan its continuation row — which is
    /// untouched, and whose `(RowId, SeqNo)` therefore did not change.
    ///
    /// The wrapped line is rows 4-5 of a 12-row viewport so the scan bound is
    /// actually tested: `2`, not `12` (`BUG-0071` F5 — the guarantee is the
    /// wrap run, which for a longer line can equal the viewport).
    #[gpui::test]
    fn class_delta_replans_the_continuation_row(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(12, 10)
            .text(4, 0, "echo \"aaa")
            .flags(4, 9, CellFlags::WRAPLINE)
            .text(5, 0, "bbb\" tail")
            .build_with_fixture();
        h.update(cx, &frame, key);
        let inside_string = h.cache.classes(5).to_vec();

        // Row 4 stops opening the quote; row 5 is not touched at all.
        rewrite_row(&mut frame, &mut fixture, 4, "echo  aaa");
        let stats = h.update(cx, &frame, key);
        assert_ne!(
            h.cache.classes(5),
            inside_string,
            "row 5 kept the classes of a string that no longer opens"
        );
        assert_eq!(stats.rows_planned, 2, "{stats:?}");
        assert_eq!(
            stats.class_rows_scanned, 3,
            "the wrap run plus the one line below it, whose role depends on the region              this one starts in (`US-0133`), not the 12-row viewport: {stats:?}"
        );
        assert_eq!(stats.class_scans, 2, "the run, and the line after it");
    }

    /// A logical line longer than the viewport: the bound is still the wrap
    /// run, and the wrap run is then the whole viewport (`BUG-0071` F5). The
    /// honest guarantee, asserted rather than claimed away.
    #[gpui::test]
    fn a_line_longer_than_the_viewport_scans_the_viewport(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 10).build_with_fixture();
        // No newline anywhere: the whole viewport is the tail of one wrap run.
        fixture.feed(format!("echo \"{}\"", "x".repeat(300)).as_bytes());
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, key);

        // One keystroke on the last row of that run.
        fixture.feed(b"z");
        resnapshot(&mut frame, &mut fixture);
        let stats = h.update(cx, &frame, key);
        assert_eq!(stats.rows_candidate, 1, "one row changed: {stats:?}");
        assert_eq!(stats.class_scans, 1, "still one scan: {stats:?}");
        assert_eq!(
            stats.class_rows_scanned, stats.rows_total,
            "the run is the viewport, and the bound says so: {stats:?}"
        );
    }

    /// Scrolling moves plans with their rows, and `shift` must move the classes
    /// with them too. Without the `class_prev` rotation a row keeps the colours
    /// of whatever row used to sit at its index (`BUG-0071` F7).
    #[gpui::test]
    fn scrolling_keeps_the_classes_with_their_rows(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(5, 24).build_with_fixture();
        fixture.feed(
            b"user@host:~$ echo \"a wrapped string that runs past the row\"\r\nplain\r\nmore\r\nlast\r\ntail\r\ntail2",
        );
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, key);
        assert_classes_match_a_full_rescan(&mut h, &frame, key, cx, "before scrolling");

        for back in 1..=3 {
            fixture.scroll_back(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, key);
            assert_classes_match_a_full_rescan(
                &mut h,
                &frame,
                key,
                cx,
                &format!("scrolled back {back}"),
            );
        }
        for fwd in 1..=3 {
            fixture.scroll_forward(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, key);
            assert_classes_match_a_full_rescan(
                &mut h,
                &frame,
                key,
                cx,
                &format!("scrolled forward {fwd}"),
            );
        }
    }

    // ── Roles and the exit-code tint (`US-0133`) ───────────────────────────

    /// A viewport of two command blocks, marked the way an integrated shell
    /// marks them: prompt, the typed command after `OSC 133;B`, its output after
    /// `OSC 133;C`, then the next prompt.
    fn two_marked_blocks() -> FrameBuilder {
        // Row 0 is the tail of an older block: a prompt is a line the OSC 133
        // region *transitions into*, and the viewport's top line has no
        // predecessor on screen (`US-0133` rework).
        FrameBuilder::new(5, 24)
            .text(0, 0, "older output")
            .mark(0, 0..12, Semantic::Output)
            .text(1, 0, "user@host:~$ ls")
            .mark(1, 0..13, Semantic::Prompt)
            .mark(1, 13..15, Semantic::Input)
            .text(2, 0, "a.txt")
            .mark(2, 0..5, Semantic::Output)
            .text(3, 0, "b.txt")
            .mark(3, 0..5, Semantic::Output)
            .text(4, 0, "user@host:~$ ")
            .mark(4, 0..13, Semantic::Prompt)
    }

    /// The roles a frame derives ride the class pass, so they cover exactly the
    /// rows the class and URL passes already scan — never more.
    #[gpui::test]
    fn roles_ride_the_class_rescan(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(12, 10)
            .text(4, 0, "echo \"aaa")
            .flags(4, 9, CellFlags::WRAPLINE)
            .text(5, 0, "bbb\" tail")
            .build_with_fixture();
        h.update(cx, &frame, key);
        rewrite_row(&mut frame, &mut fixture, 4, "echo  aaa");
        let stats = h.update(cx, &frame, key);
        assert_eq!(
            stats.url_rows_scanned, 2,
            "the URL bound is exactly the wrap run, as `US-0092` states: {stats:?}"
        );
        assert_eq!(
            stats.class_rows_scanned, 3,
            "the wrap run plus the one logical line whose role depends on it: {stats:?}"
        );
        for r in 0..12 {
            assert_eq!(h.cache.role(r), None, "row {r} carries no mark");
        }
    }

    /// Every row of a marked block gets its logical line's role, and an unmarked
    /// session keeps `None` per row.
    #[gpui::test]
    fn roles_are_read_from_the_marks(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::semantic();
        let frame = two_marked_blocks().build();
        h.update(cx, &frame, semantic_key());
        assert_eq!(h.cache.role(0), Some(RowRole::Output));
        assert_eq!(h.cache.role(1), Some(RowRole::Prompt));
        assert_eq!(h.cache.role(2), Some(RowRole::Output));
        assert_eq!(h.cache.role(3), Some(RowRole::Output));
        assert_eq!(h.cache.role(4), Some(RowRole::Prompt));
    }

    /// A row whose role changes is replanned even though its own classes might
    /// not move: the role is an input of the plan (`US-0134` paints from it).
    #[gpui::test]
    fn a_role_change_replans_its_run(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        const TEXT: &str = "plain output";
        let (mut frame, mut fixture) = FrameBuilder::new(4, 24)
            .text(1, 0, TEXT)
            .build_with_fixture();
        h.update(cx, &frame, key);
        assert_eq!(h.cache.role(1), None);

        // The same text, now inside a marked output region.
        fixture.begin_batch();
        for (col, ch) in TEXT.chars().enumerate() {
            fixture.write(
                1,
                col,
                &FixtureCell {
                    ch,
                    semantic: Semantic::Output,
                    ..FixtureCell::default()
                },
            );
        }
        resnapshot(&mut frame, &mut fixture);
        let stats = h.update(cx, &frame, key);
        assert_eq!(h.cache.role(1), Some(RowRole::Output));
        assert_eq!(stats.rows_planned, 1, "only its own run: {stats:?}");
    }

    /// The tint reaches the prompt of the most recently completed block, and
    /// only that one.
    #[gpui::test]
    fn the_tint_lands_on_the_completed_block(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let frame = two_marked_blocks().build();
        h.update(cx, &frame, key);
        assert_eq!(h.cache.tint(), None, "no exit code reported yet");

        if let Some(overlay) = h.semantic.as_mut() {
            overlay.set_exit_code(Some(0));
        }
        let stats = h.update(cx, &frame, key);
        assert_eq!(
            h.cache.tint(),
            Some((1..2, 0)),
            "the older prompt, not the newest one"
        );
        assert_eq!(stats.rows_planned, 1, "only the row it moved on to");
        assert_eq!(
            stats.class_rows_scanned, 0,
            "the tint needs no rescan: {stats:?}"
        );
    }

    /// A real OSC 133 stream, fed to the engine rather than hand-marked: three
    /// plain lines, a prompt with its command, and the command's output.
    const MARKED_STREAM: &[u8] = b"one\r\ntwo\r\nthree\r\n\x1b]133;A\x1b\\user@host:~$ \x1b]133;B\x1b\\ls\r\n\x1b]133;C\x1b\\a.txt\r\nb.txt\r\n";

    /// A full `A`/`B`/`C`/`D` stream where the command printed nothing, so the
    /// next prompt lands directly under it (`RV-MAJ-1`). Fed to the engine.
    const BACK_TO_BACK_STREAM: &[u8] = b"one\r\n\x1b]133;A\x1b\\user@host:~$ \x1b]133;B\x1b\\true\r\n\x1b]133;C\x1b\\\x1b]133;D;0\x1b\\\x1b]133;A\x1b\\user@host:~$ \x1b]133;B\x1b\\";

    /// Two prompts on adjacent rows are both prompts, and the older one takes
    /// the tint its `OSC 133;D;0` reported.
    #[gpui::test]
    fn back_to_back_prompts_are_both_marked_and_the_older_one_is_tinted(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        if let Some(overlay) = h.semantic.as_mut() {
            overlay.set_exit_code(Some(0));
        }
        let (mut frame, mut fixture) = FrameBuilder::new(6, 32).build_with_fixture();
        fixture.feed(BACK_TO_BACK_STREAM);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, key);
        assert_eq!(
            h.cache.role(1),
            Some(RowRole::Prompt),
            "the command that ran"
        );
        assert_eq!(
            h.cache.role(2),
            Some(RowRole::Prompt),
            "the prompt it left behind, with no output between them"
        );
        assert_eq!(
            h.cache.tint(),
            Some((1..2, 0)),
            "two prompt runs, so the older one is the completed block"
        );
    }

    /// The viewport's top line has no predecessor on screen, so its region is
    /// unknown and it is never a marked prompt — and that answer does not
    /// flicker: scrolling the same prompt away from the top edge marks it,
    /// scrolling it back unmarks it, every time.
    #[gpui::test]
    fn a_prompt_at_the_top_of_the_viewport_is_unmarked_and_stays_so(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(4, 24).build_with_fixture();
        fixture.feed(MARKED_STREAM);
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, key);
        assert_eq!(
            h.cache.role(0),
            None,
            "the prompt is the top line: no predecessor on screen"
        );

        for _ in 0..2 {
            fixture.scroll_back(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, key);
            assert_eq!(
                h.cache.role(1),
                Some(RowRole::Prompt),
                "one row down, the predecessor is visible and the region transitions"
            );

            fixture.scroll_forward(1);
            resnapshot(&mut frame, &mut fixture);
            h.update(cx, &frame, key);
            assert_eq!(h.cache.role(0), None, "and back to unknown at the top");
        }
    }

    /// An exit code that arrives with a frame that changed nothing must not be
    /// dropped: it is the one tint input that is not in the frame.
    #[gpui::test]
    fn an_exit_code_on_an_unchanged_frame_is_not_dropped(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let key = semantic_key();
        let mut h = Harness::semantic();
        let (mut frame, mut fixture) = FrameBuilder::new(6, 24).build_with_fixture();
        // Two marked blocks: a line, a prompt with its command, its output, and
        // the next prompt.
        fixture.feed(
            b"one\r\n\x1b]133;A\x1b\\user@host:~$ \x1b]133;B\x1b\\ls\r\n\x1b]133;C\x1b\\a.txt\r\n\x1b]133;A\x1b\\user@host:~$ \x1b]133;B\x1b\\",
        );
        resnapshot(&mut frame, &mut fixture);
        h.update(cx, &frame, key);
        assert_eq!(h.cache.role(1), Some(RowRole::Prompt));
        assert_eq!(h.cache.tint(), None, "no exit code reported yet");

        // Nothing moved: the frame really is `Unchanged`.
        resnapshot(&mut frame, &mut fixture);
        let stats = h.update(cx, &frame, key);
        assert_eq!(stats.frames_unchanged, 1, "{stats:?}");

        // `OSC 133;D` arrives with no redraw behind it.
        if let Some(overlay) = h.semantic.as_mut() {
            overlay.set_exit_code(Some(3));
        }
        resnapshot(&mut frame, &mut fixture);
        let stats = h.update(cx, &frame, key);
        assert_eq!(
            stats.frames_unchanged, 0,
            "a new exit code is not 'nothing to do': {stats:?}"
        );
        assert_eq!(h.cache.tint(), Some((1..2, 3)));
    }

    /// A viewport with one prompt on it — a command still running — is not
    /// tinted with the code of whatever finished before.
    #[gpui::test]
    fn a_single_prompt_is_never_tinted(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        let mut h = Harness::semantic();
        if let Some(overlay) = h.semantic.as_mut() {
            overlay.set_exit_code(Some(101));
        }
        let frame = FrameBuilder::new(3, 24)
            .text(0, 0, "user@host:~$ sleep")
            .mark(0, 0..13, Semantic::Prompt)
            .mark(0, 13..18, Semantic::Input)
            .text(1, 0, "working")
            .mark(1, 0..7, Semantic::Output)
            .build();
        h.update(cx, &frame, semantic_key());
        assert_eq!(h.cache.tint(), None);
    }

    /// The incremental classes of every row must equal a from-scratch scan of
    /// the same frame by a cache that has never seen anything else.
    fn assert_classes_match_a_full_rescan(
        h: &mut Harness,
        frame: &Frame,
        key: StyleKey,
        cx: &mut VisualTestContext,
        when: &str,
    ) {
        let mut fresh = Harness::semantic();
        fresh.update(cx, frame, key);
        for r in 0..usize::from(frame.size().rows) {
            assert_eq!(
                h.cache.classes(r),
                fresh.cache.classes(r),
                "row {r} classes drifted {when}"
            );
        }
    }
}
