//! Synchronized output (mode 2026): frames are skipped, nothing is buffered.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
//! section "Synchronized output".
//!
//! The reference holds up to 2 MiB of **unapplied bytes** while an update is
//! open, which is a memory amplifier a hostile stream can aim at us. Here the
//! bytes are applied as they arrive and the renderer simply does not look:
//! a program that opens an update and never closes it costs frames, not memory.
//!
//! Both deadlines are evaluated against the `now` the caller passes (R-11), so a
//! replay with a synthetic clock is deterministic and these tests never flake.

use std::time::{Duration, Instant};

/// How long one `CSI ? 2026 h` holds frames back before the renderer refreshes
/// anyway. A well-behaved program refreshes it every frame.
pub const SYNC_REFRESH: Duration = Duration::from_millis(150);

/// How long the mode may stay set at all before the engine forces it off.
pub const SYNC_WATCHDOG: Duration = Duration::from_secs(1);

/// The open synchronized update, if any.
///
/// Owned by the terminal (`US-0076` puts it on `Terminal`), passed to the render
/// state through [`crate::render::EngineView`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SyncState {
    /// When the current update stops holding frames back.
    open_until: Option<Instant>,
    /// When the mode is forced off, set on the **first** open and not refreshed.
    watchdog: Option<Instant>,
}

impl SyncState {
    pub fn new() -> SyncState {
        SyncState::default()
    }

    /// `CSI ? 2026 h`. A further `h` refreshes the frame deadline only — the
    /// watchdog keeps counting from the first open, or a program could hold the
    /// screen still forever by repeating itself.
    pub fn begin(&mut self, now: Instant) {
        self.open_until = Some(now + SYNC_REFRESH);
        if self.watchdog.is_none() {
            self.watchdog = Some(now + SYNC_WATCHDOG);
        }
    }

    /// `CSI ? 2026 l`.
    pub fn end(&mut self) {
        self.open_until = None;
        self.watchdog = None;
    }

    /// What `DECRQM` reports: the real state, not a guess.
    pub fn is_set(&self) -> bool {
        self.open_until.is_some()
    }

    /// Whether this frame is skipped, closing the update first if the watchdog
    /// has run out.
    pub fn suppresses_frame(&mut self, now: Instant) -> bool {
        let Some(open_until) = self.open_until else {
            return false;
        };
        if self.watchdog.is_some_and(|deadline| now >= deadline) {
            self.end();
            return false;
        }
        now < open_until
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
