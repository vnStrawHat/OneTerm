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
//! **It is a count of waiters, not a one-shot flag, and the asking does not
//! clear it** (`US-0082` rework, `US-0083` gap 6). A flag cleared by the pump's
//! ask can be consumed in the window between "the renderer raised" and "the
//! renderer is queued on the mutex": the pump then sees nothing waiting, keeps
//! the engine until the transport runs dry, and the frame starves for the whole
//! flood (measured 3/3 at > 5 s). Only the waiter itself knows when it no
//! longer needs the yield, so only the waiter clears — by [`Demand::release`],
//! once it holds the lock.
//!
//! This is the **adapter's** primitive, not the engine's: it is reachable from
//! no engine type, holds the crate's only atomic, and lives here so the contract
//! and its test have one home until `US-0081` moves the pump loop over.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many renderers are waiting for the engine, as a hand-off between the
/// render thread and the pump thread.
#[derive(Clone, Debug, Default)]
pub struct Demand(Arc<AtomicUsize>);

impl Demand {
    pub fn new() -> Demand {
        Demand::default()
    }

    /// The renderer wants the lock. Call it **before** blocking on the lock,
    /// and pair it with [`Demand::release`] once the lock is held.
    pub fn raise(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    /// The renderer is in (or has given up): it no longer needs the pump to
    /// yield for it.
    pub fn release(&self) {
        // Saturating: an unpaired release must not wrap the count into "the
        // whole world is waiting".
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |waiting| {
                waiting.checked_sub(1)
            });
    }

    /// Is anyone waiting? The pump asks at a chunk boundary and yields if so.
    ///
    /// Reading does **not** clear: the demand stands until the waiter that
    /// raised it holds the lock.
    pub fn is_raised(&self) -> bool {
        self.0.load(Ordering::Acquire) > 0
    }
}
