//! `LineAccounting` — absolute line count decoupled from the scrollback cap.
//!
//! The total line count stops growing once the scrollback is full, but the
//! gutter line numbers must keep counting. The pump owns one instance and
//! feeds it after every `Terminal::feed`; the result is published to
//! `SharedState::absolute_line_count` once per batch (no shared lock on the
//! hot path).
//!
//! Heuristic:
//! - `total_lines` grew → scrollback not yet full, add the growth;
//! - `total_lines` unchanged and larger than the screen → scrollback is full,
//!   count the newlines in the batch as dropped lines;
//! - `total_lines` shrank → clear / alt-screen / resize, restart from it.
//!
//! The whole file is deleted at `US-0082`: the engine already counts output
//! lines exactly (`Terminal::lines_produced()`, P15), and the only reason this
//! heuristic survives the shim is that changing what the gutter shows is a
//! behaviour change, which this packet forbids.

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

    /// Account for a parse batch of `bytes` that was just fed into a terminal
    /// now holding `total_lines` rows over a `screen_lines`-row viewport.
    pub fn observe(&mut self, total_lines: usize, screen_lines: usize, bytes: &[u8]) {
        let total_after = total_lines;
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
