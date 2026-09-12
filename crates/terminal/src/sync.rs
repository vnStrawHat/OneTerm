//! The adapter's lock.
//!
//! `oneterm-vt` holds no lock and no interior mutability: it takes `&mut self`
//! and the embedder picks the synchronisation
//! (`docs/spec-intakes/IN-0029-vt-engine/high-level-design.md`, "Threading and
//! locking"). That embedder is this crate, and the choice is
//! `parking_lot::FairMutex` — already in `Cargo.lock`, and the one primitive
//! `alacritty_terminal::sync::FairMutex` offers on top of it (`lease()`) has no
//! call site in OneTerm (`research/api-surface.md` § 3.9).
//!
//! The newtype exists for one reason: the two backend read loops call
//! `lock_unfair()` / `try_lock_unfair()`, and those loops belong to `US-0083`
//! and `US-0084`, not to this packet.

use parking_lot::FairMutexGuard;

/// A fair mutex with the method set the backend read loops already call.
#[derive(Debug, Default)]
pub struct FairMutex<T>(parking_lot::FairMutex<T>);

impl<T> FairMutex<T> {
    pub fn new(value: T) -> FairMutex<T> {
        FairMutex(parking_lot::FairMutex::new(value))
    }

    /// Acquire, waiting behind whoever the fair hand-off favours.
    pub fn lock(&self) -> FairMutexGuard<'_, T> {
        self.0.lock()
    }

    /// Acquire without waiting for the fair hand-off.
    ///
    // ponytail: `parking_lot`'s fairness lives in `unlock`, so there is no
    // unfair acquire to call and this is `lock()`. The call sites that ask for
    // one are `crates/local-shell/src/event_loop.rs`'s read loop, which
    // `US-0083` rewrites onto the demand/yield handshake; upgrade path is to
    // delete both methods with those call sites.
    pub fn lock_unfair(&self) -> FairMutexGuard<'_, T> {
        self.0.lock()
    }

    /// `None` when someone else holds it.
    pub fn try_lock_unfair(&self) -> Option<FairMutexGuard<'_, T>> {
        self.0.try_lock()
    }
}
