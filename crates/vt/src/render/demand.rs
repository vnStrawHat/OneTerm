//! The renderer's "let me in" flag.
//!
//! Design: `docs/spec-intakes/IN-0029-vt-engine/low-level-design/damage-and-render-state.md`
//! section "Fairness and reply latency" (R-37).
//!
//! A fair mutex is not enough under sustained output: a pump thread that
//! unlocks and immediately relocks still beats a sleeping waiter. So the render
//! path raises this flag and the pump tests it at every chunk boundary — after
//! the batch's `Reply` bytes have left, never before — and yields.
//!
//! This is the **adapter's** primitive, not the engine's: it is reachable from
//! no engine type, holds the crate's only atomic, and lives here so the contract
//! and its test have one home until `US-0081` moves the pump loop over.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A one-bit hand-off between the render thread and the pump thread.
#[derive(Clone, Debug, Default)]
pub struct Demand(Arc<AtomicBool>);

impl Demand {
    pub fn new() -> Demand {
        Demand::default()
    }

    /// The renderer wants the lock.
    pub fn raise(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// The pump asks whether anyone is waiting, and clears the flag.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }

    /// Read without clearing, for diagnostics and tests.
    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
