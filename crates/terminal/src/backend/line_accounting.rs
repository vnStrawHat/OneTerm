//! `LineAccounting` — absolute line count decoupled from the scrollback cap.
//!
//! `Term::total_lines()` stops growing once the scrollback is full, but the
//! gutter line numbers must keep counting. The pump owns one instance and
//! feeds it after every `Processor::advance`; the result is published to
//! `SharedState::absolute_line_count` once per batch (no shared lock on the
//! hot path).
//!
//! Heuristic (PERF-19 tracks replacing the `\n` scan by a grid counter in the
//! vendored fork):
//! - `total_lines` grew → scrollback not yet full, add the growth;
//! - `total_lines` unchanged and larger than the screen → scrollback is full,
//!   count the newlines in the batch as dropped lines;
//! - `total_lines` shrank → clear / alt-screen / resize, restart from it.

use alacritty_terminal::grid::Dimensions;

/// Absolute-line counter for one session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LineAccounting {
    absolute: usize,
    prev_total: usize,
}

impl LineAccounting {
    /// Start counting from zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Account for a parse batch of `bytes` that was just fed into `term`.
    pub fn observe<D: Dimensions>(&mut self, term: &D, bytes: &[u8]) {
        let total_after = term.total_lines();
        let screen_lines = term.screen_lines();
        if total_after > self.prev_total {
            self.absolute += total_after - self.prev_total;
        } else if total_after == self.prev_total && total_after > screen_lines {
            self.absolute += bytes.iter().filter(|&&b| b == b'\n').count();
        } else if total_after < self.prev_total {
            self.absolute = total_after;
        }
        self.prev_total = total_after;
    }

    /// Absolute lines output since spawn.
    pub fn absolute(&self) -> usize {
        self.absolute
    }
}
