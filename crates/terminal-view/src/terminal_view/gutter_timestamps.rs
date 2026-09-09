//! Grow-only per-line timestamps for the gutter.
//!
//! [`GutterTimestamps`] stamps each scrollback line with the wall-clock time it
//! first appeared and never overwrites it. Stamping runs when output arrives
//! (the single stamper — see `TerminalView::handle_event`), so inactive tabs
//! keep accurate times too. Formatting happens in the render engine for the
//! rows actually painted.

use std::collections::VecDeque;
use std::rc::Rc;

use chrono::Timelike as _;
use oneterm_terminal::TerminalInfo;

use crate::render::state::SecondsOfDay;

/// Current local time as seconds since midnight.
fn now_seconds_of_day() -> SecondsOfDay {
    chrono::Local::now().num_seconds_from_midnight()
}

/// Per-line render timestamps, keyed by each line's **absolute index**.
///
/// `times[j]` is the render time of the line whose absolute index (0-based) is
/// `base + j`. Grow-only: each line is stamped exactly once and never
/// overwritten (see [`GutterTimestamps::update`]).
#[derive(Debug, Default)]
pub(super) struct GutterTimestamps {
    /// Timestamps shared with the render inputs via `Rc`.
    times: Rc<VecDeque<SecondsOfDay>>,
    /// Absolute index (0-based) of `times[0]` — the oldest line still tracked.
    /// Increases as old lines leave the scrollback.
    base: usize,
    /// `clear_epoch` from the most recent update — when it changes (screen
    /// `clear`), the timestamps are reset so new content is stamped with the
    /// current time.
    last_clear_epoch: usize,
}

impl GutterTimestamps {
    /// The shared timestamp storage (cheap clone for the render inputs).
    pub(super) fn times(&self) -> Rc<VecDeque<SecondsOfDay>> {
        self.times.clone()
    }

    /// Absolute index of `times()[0]`.
    pub(super) fn base(&self) -> usize {
        self.base
    }

    /// Stamp newly appeared lines with the current local time.
    pub(super) fn update(&mut self, info: &TerminalInfo) {
        self.update_with(info, now_seconds_of_day());
    }

    /// Update the timestamps using a **grow-only** model keyed by each line's
    /// absolute index.
    ///
    /// Each line is assigned a timestamp exactly **once** — on the first frame it
    /// appears — and is **never overwritten**. This is what resists ConPTY
    /// repaint / reflow: those operations make `total_lines` (and therefore
    /// `absolute_line_count`) temporarily dip. Clearing and refilling with `now`
    /// on every dip would make every line jump to the same time; here a dip
    /// simply means "add nothing".
    fn update_with(&mut self, info: &TerminalInfo, now: SecondsOfDay) {
        let total = info.total_lines;
        let absolute = info.absolute_line_count;

        // `Rc::make_mut` is O(1) when the Rc is unique (the previous frame's
        // inputs copy has been replaced by render time).
        let times = Rc::make_mut(&mut self.times);

        // `clear` resets the absolute line counter → new content REUSES old
        // indices. Kept timestamps would make new lines hit stale entries.
        if info.clear_epoch != self.last_clear_epoch {
            self.last_clear_epoch = info.clear_epoch;
            times.clear();
            self.base = absolute.saturating_sub(total);
        }

        // Number of lines that ALREADY HAVE CONTENT (high-water mark).
        //
        // `absolute_line_count` is "inflated" to the bottom of the viewport
        // because `total_lines = history + screen_lines` always includes the
        // EMPTY lines below the cursor. Stamping up to `absolute` would give
        // those empty lines the current time; when later output overwrites
        // them they keep the old time — "a block of lines carries the wrong
        // time". The content mark follows the last line **with content**, not
        // just the cursor: TUIs and progress bars that use cursor-up have
        // content BELOW the cursor.
        let content_row = info.cursor_line.max(info.last_content_line).max(0) as usize;
        let content_high = absolute
            .saturating_sub(info.num_lines)
            .saturating_add(content_row + 1)
            .min(absolute);

        // Hard reset only when new content starts BEFORE the oldest tracked
        // line (the absolute counter was fully reset). ConPTY repaint/reflow
        // only fluctuates within existing content, so it never hits this.
        if absolute < self.base {
            times.clear();
            self.base = absolute.saturating_sub(total);
        }
        if times.is_empty() {
            self.base = absolute.saturating_sub(total);
        }

        // Stamp the new lines WITH CONTENT. Grow-only: a temporary dip pushes
        // nothing; empty lines below the cursor wait until content reaches them.
        let covered = self.base + times.len();
        if content_high > covered {
            let new_lines = content_high - covered;
            times.reserve(new_lines);
            for _ in 0..new_lines {
                times.push_back(now);
            }
        }

        // Drop timestamps of lines that left the scrollback to bound memory.
        let oldest = absolute.saturating_sub(total);
        if oldest > self.base {
            let drop = (oldest - self.base).min(times.len());
            for _ in 0..drop {
                times.pop_front();
            }
            self.base += drop;
        }
    }
}

#[cfg(test)]
mod tests {
    use oneterm_terminal::TerminalInfo;

    use super::GutterTimestamps;

    /// A 10-row viewport with `history` scrolled-off lines and content down to
    /// `content_row` (0-based screen row).
    fn info(history: usize, content_row: i32, clear_epoch: usize) -> TerminalInfo {
        let num_lines = 10;
        TerminalInfo {
            total_lines: history + num_lines,
            absolute_line_count: history + num_lines,
            cursor_line: content_row,
            last_content_line: content_row,
            num_lines,
            num_cols: 80,
            display_offset: 0,
            clear_epoch,
        }
    }

    fn stamps(g: &GutterTimestamps) -> Vec<u32> {
        g.times().iter().copied().collect()
    }

    #[test]
    fn stamps_only_lines_with_content() {
        let mut g = GutterTimestamps::default();
        g.update_with(&info(0, 2, 0), 1);
        assert_eq!(g.base(), 0);
        assert_eq!(stamps(&g), [1, 1, 1]);
    }

    #[test]
    fn stamps_are_grow_only_and_never_overwritten() {
        let mut g = GutterTimestamps::default();
        g.update_with(&info(0, 1, 0), 1);
        // The cursor moves down two rows: only the two new rows get "2".
        g.update_with(&info(0, 3, 0), 2);
        assert_eq!(stamps(&g), [1, 1, 2, 2]);
        // A temporary dip (ConPTY reflow) adds nothing and keeps old stamps.
        g.update_with(&info(0, 0, 0), 3);
        assert_eq!(stamps(&g), [1, 1, 2, 2]);
    }

    #[test]
    fn stamps_follow_lines_into_history() {
        let mut g = GutterTimestamps::default();
        g.update_with(&info(0, 9, 0), 1);
        // Five lines scroll into history; the viewport is full again.
        g.update_with(&info(5, 9, 0), 2);
        assert_eq!(g.base(), 0);
        assert_eq!(g.times().len(), 15);
        assert_eq!(g.times()[9], 1);
        assert_eq!(g.times()[10], 2);
    }

    #[test]
    fn drops_stamps_that_left_the_scrollback() {
        let mut g = GutterTimestamps::default();
        g.update_with(&info(0, 9, 0), 1);
        // The scrollback holds only 12 lines but 20 have been output: the
        // oldest 8 stamps are dropped and `base` advances.
        let mut i = info(10, 9, 0);
        i.total_lines = 12;
        g.update_with(&i, 2);
        assert_eq!(g.base(), 8);
        assert_eq!(g.times().len(), 12);
    }

    #[test]
    fn clear_epoch_change_resets_the_stamps() {
        let mut g = GutterTimestamps::default();
        g.update_with(&info(0, 5, 0), 1);
        // `clear` restarts the absolute counter; the new content must be
        // stamped with the new time, not the stale entries.
        g.update_with(&info(0, 0, 1), 2);
        assert_eq!(stamps(&g), [2]);
    }

    #[test]
    fn absolute_counter_reset_hard_resets() {
        let mut g = GutterTimestamps::default();
        // 60 lines output, scrollback trimmed to 20 → base advances to 40.
        let mut trimmed = info(50, 9, 0);
        trimmed.total_lines = 20;
        g.update_with(&trimmed, 1);
        assert_eq!(g.base(), 40);
        // The absolute counter restarts below the oldest tracked line.
        g.update_with(&info(0, 0, 0), 2);
        assert_eq!(g.base(), 0);
        assert_eq!(stamps(&g), [2]);
    }
}
