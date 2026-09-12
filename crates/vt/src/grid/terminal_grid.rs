//! Both screens, the shared anchor list and the counters that outlive a screen.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/grid-and-scrollback.md`
//! sections "Alternate screen" and "Line counting".
//!
//! This is the grid half of the future `Terminal` (`US-0079`), which will own
//! one of these next to the parser, the interner and the mode table. It exists
//! now because three things have no other owner: the row-id space shared by both
//! screens, the anchor list every primitive updates, and `lines_produced`, which
//! must not jump when the alternate screen is entered.

use crate::grid::anchor::Anchors;
use crate::grid::row::SeqNo;
use crate::grid::screen::{DisplayClear, PrintMode, Screen, ScreenKind, ScrollReport};
use crate::grid::{Pos, RowId, ScrollRegion, Size};
use crate::intern::Interner;
use crate::reflow::{ResizeOutcome, ResizePolicy};

/// The primary and alternate screens, plus what they share.
#[derive(Debug)]
pub struct TerminalGrid {
    primary: Screen,
    alt: Screen,
    alt_active: bool,
    anchors: Anchors,
    /// Output lines, not rows created (R-05): the number the gutter shows.
    lines_produced: u64,
    seq: SeqNo,
}

impl TerminalGrid {
    pub fn new(size: Size, scrollback_limit: u32) -> TerminalGrid {
        let mut anchors = Anchors::new();
        let primary = Screen::new(ScreenKind::Primary, size, scrollback_limit, &mut anchors);
        // The alternate screen has no scrollback, ever.
        let alt = Screen::new(ScreenKind::Alternate, size, 0, &mut anchors);
        TerminalGrid {
            primary,
            alt,
            alt_active: false,
            anchors,
            lines_produced: 0,
            seq: SeqNo::default(),
        }
    }

    // ── Access ──────────────────────────────────────────────────────────────

    pub fn screen(&self) -> &Screen {
        if self.alt_active {
            &self.alt
        } else {
            &self.primary
        }
    }

    pub fn screen_mut(&mut self) -> &mut Screen {
        self.active().0
    }

    /// The active screen and the anchor list, borrowed as the disjoint fields
    /// they are: every primitive needs both at once.
    fn active(&mut self) -> (&mut Screen, &mut Anchors) {
        let screen = if self.alt_active {
            &mut self.alt
        } else {
            &mut self.primary
        };
        (screen, &mut self.anchors)
    }

    pub fn primary(&self) -> &Screen {
        &self.primary
    }

    pub fn alt(&self) -> &Screen {
        &self.alt
    }

    pub fn alt_active(&self) -> bool {
        self.alt_active
    }

    pub fn anchors(&self) -> &Anchors {
        &self.anchors
    }

    pub fn anchors_mut(&mut self) -> &mut Anchors {
        &mut self.anchors
    }

    /// Which screen a row id belongs to. The two runs never overlap, so this is
    /// unambiguous.
    pub fn screen_of(&self, id: RowId) -> &Screen {
        if id >= RowId::ALT_ORIGIN {
            &self.alt
        } else {
            &self.primary
        }
    }

    pub fn lines_produced(&self) -> u64 {
        self.lines_produced
    }

    /// Start a new batch. One stamp per `feed`, not per mutation.
    pub fn begin_batch(&mut self) -> SeqNo {
        self.seq = self.seq.next();
        self.primary.set_seq(self.seq);
        self.alt.set_seq(self.seq);
        self.seq
    }

    pub fn seq(&self) -> SeqNo {
        self.seq
    }

    // ── Primitives, with the output-line counter maintained ─────────────────

    /// `LF`, `IND`, `NEL`: one output line each, whether or not the screen
    /// scrolled. An implicit wrap produces none.
    pub fn linefeed(&mut self) -> Option<ScrollReport> {
        let (screen, anchors) = self.active();
        let report = screen.linefeed(anchors);
        self.lines_produced = self.lines_produced.saturating_add(1);
        report
    }

    pub fn reverse_index(&mut self) -> Option<ScrollReport> {
        let (screen, anchors) = self.active();
        screen.reverse_index(anchors)
    }

    /// `SU`. Rows that enter scrollback are output lines; rows that only move
    /// inside a region are not.
    pub fn scroll_up(&mut self, region: ScrollRegion, n: u16) -> ScrollReport {
        let (screen, anchors) = self.active();
        let report = screen.scroll_up(region, n, anchors);
        self.count_history_rows(&report);
        report
    }

    /// `SD`. Never touches history, so it never counts (trap 18).
    pub fn scroll_down(&mut self, region: ScrollRegion, n: u16) -> ScrollReport {
        let (screen, anchors) = self.active();
        screen.scroll_down(region, n, anchors)
    }

    pub fn insert_lines(&mut self, n: u16) -> Option<ScrollReport> {
        let (screen, anchors) = self.active();
        screen.insert_lines(n, anchors)
    }

    pub fn delete_lines(&mut self, n: u16) -> Option<ScrollReport> {
        let (screen, anchors) = self.active();
        let report = screen.delete_lines(n, anchors);
        if let Some(report) = &report {
            self.count_history_rows(report);
        }
        report
    }

    fn count_history_rows(&mut self, report: &ScrollReport) {
        self.lines_produced = self
            .lines_produced
            .saturating_add(u64::from(report.history_rows));
    }

    /// An implicit wrap: **not** an output line.
    pub fn wrapline(&mut self) {
        let (screen, anchors) = self.active();
        screen.wrapline(anchors);
    }

    pub fn print(&mut self, c: char, mode: PrintMode, interner: &mut Interner) {
        let (screen, anchors) = self.active();
        screen.print(c, mode, interner, anchors);
    }

    /// `HT`. `autowrap` is `DECAWM`, which decides whether a pending wrap turns
    /// into a line break or is merely consumed (trap 3).
    pub fn put_tab(&mut self, count: u16, autowrap: bool) {
        let (screen, anchors) = self.active();
        screen.put_tab(count, autowrap, anchors);
    }

    pub fn erase_display(
        &mut self,
        mode: DisplayClear,
        interner: &Interner,
    ) -> Option<ScrollReport> {
        let (screen, anchors) = self.active();
        screen.erase_display(mode, anchors, interner)
    }

    /// `RIS`. Ids are never reset and neither is `lines_produced`: the gutter's
    /// numbers keep their meaning across a clear.
    pub fn reset(&mut self) {
        self.primary.reset(&mut self.anchors);
        self.alt.reset(&mut self.anchors);
        self.alt_active = false;
        self.sync_anchors();
        self.assert_integrity(None);
    }

    /// Resize both screens under one policy (`US-0077`).
    ///
    /// The primary screen reflows; the alternate screen never does (trap 29).
    /// `KeepViewportTop` always corrects the **primary** screen, including while
    /// a TUI holds the alternate one, which is what `DEC-0008` requires.
    pub fn resize(&mut self, size: Size, policy: ResizePolicy) -> ResizeOutcome {
        let outcome = crate::reflow::resize(
            &mut self.primary,
            &mut self.alt,
            size,
            policy,
            &mut self.anchors,
        );
        self.assert_integrity(None);
        outcome
    }

    /// The user edited the configured scrollback depth. Only the primary screen
    /// has history, so only it is rehomed.
    pub fn set_scrollback_limit(&mut self, limit: u32) {
        self.primary.set_scrollback_limit(limit, &mut self.anchors);
        self.sync_anchors();
        self.assert_integrity(None);
    }

    // ── Alternate screen ────────────────────────────────────────────────────

    /// `CSI ? 1049 h/l`.
    ///
    /// Trap 14: entering takes the primary `DECSC` slot, which is exactly what
    /// `? 1049` means; leaving takes none of that branch, so the primary returns
    /// with the cursor and saved cursor it had on entry.
    pub fn swap_alt(&mut self) {
        if !self.alt_active {
            let index = self.primary.cursor_row_index();
            let cursor = *self.primary.cursor();
            // The reference keeps one scroll region and one tab table on `Term`,
            // shared by both screens, so entering does not reset either; copying
            // them across in both directions reproduces that.
            self.alt.adopt_region_and_tabs(&self.primary);
            // The cursor carries its row INDEX across, never a row id from the
            // other screen (R-04). It is installed *before* the wipe, so the
            // wipe is a background erase with the entering template.
            let row = self.alt.row_of_index(index);
            let alt_cursor = self.alt.cursor_mut();
            *alt_cursor = cursor;
            alt_cursor.pos = Pos {
                row,
                col: cursor.pos.col,
            };
            self.alt.clear_all_rows(&mut self.anchors);
            self.primary.save_cursor();
        } else {
            self.primary.adopt_region_and_tabs(&self.alt);
        }
        self.alt_active = !self.alt_active;
        self.sync_anchors();
        self.assert_integrity(None);
    }

    // ── Anchors and integrity ───────────────────────────────────────────────

    /// Push both screens' cached cursor and viewport fields back into the anchor
    /// list. Called at the end of a batch, a resize and a screen swap.
    pub fn sync_anchors(&mut self) {
        self.primary.sync_anchors(&mut self.anchors);
        self.alt.sync_anchors(&mut self.anchors);
    }

    /// The full two-screen walk (R-28): once per `feed`, `resize` and
    /// `render_update`, plus after every step of the property tests. The O(1)
    /// tier runs at the end of every mutating method.
    pub fn assert_integrity(&self, interner: Option<&Interner>) {
        if !cfg!(debug_assertions) {
            return;
        }
        self.primary.assert_integrity();
        self.alt.assert_integrity();
        debug_assert!(
            self.primary.newest() < RowId::ALT_ORIGIN,
            "the primary screen's run reached the alternate screen's"
        );
        debug_assert!(
            self.alt.oldest() >= RowId::ALT_ORIGIN,
            "the alternate screen's run reached the primary screen's"
        );
        for (_, anchor) in self.anchors.iter() {
            if !anchor.alive {
                continue;
            }
            let screen = self.screen_of(anchor.pos.row);
            debug_assert!(
                anchor.pos.row >= screen.oldest() && anchor.pos.row <= screen.newest(),
                "anchor {:?} is outside the live row range",
                anchor.kind
            );
        }
        // Counted per lane, because a lane-blind bound is exactly how one
        // screen's trim used to kill the other screen's anchors: each screen
        // registers its cursor, saved cursor and viewport top once and never
        // releases them, so three of each must always be live in its own run.
        for screen in [&self.primary, &self.alt] {
            debug_assert_eq!(
                self.anchors.live_screen_owned_in(screen.row_range()),
                3,
                "{:?} lost a screen-owned anchor",
                screen.kind()
            );
        }
        if let Some(interner) = interner {
            self.primary.assert_interned_ids_resolve(interner);
            self.alt.assert_interned_ids_resolve(interner);
        }
    }

    /// Live heap both screens own, for the memory probe.
    pub fn heap_bytes(&self) -> usize {
        self.primary.heap_bytes() + self.alt.heap_bytes()
    }
}
