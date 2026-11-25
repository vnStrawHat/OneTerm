//! `ByteBudget` — the aggregate write-payload cap both backends put in front of
//! their bounded command queue.
//!
//! A message count alone does not bound memory: 256 queued pastes of 16 MiB
//! each is 4 GiB. Both backends therefore reserve a byte total at enqueue time
//! and release it once the payload has been delivered or dropped, so a write is
//! atomic at enqueue — accepted whole, or refused with `WouldBlock` /
//! `TerminalError::QueueFull` (`docs/terminal-backend.md` §6.5).
//!
//! The ceiling is the type parameter, so **each backend keeps its own
//! constant**: `LOCAL_COMMAND_BYTE_BUDGET` and `SSH_COMMAND_BYTE_BUDGET` are
//! each backend's own policy — identical by coincidence today, and each named
//! by that backend's own tests — while only the mechanism is shared
//! (`US-0093`, R10). Carrying the limit in the type also means a second
//! reservation cannot be made against a different one by accident, and leaves
//! `Default` derivable by the structs that hold a budget.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Aggregate reserved bytes, capped at `LIMIT`.
#[derive(Debug, Default)]
pub struct ByteBudget<const LIMIT: usize>(AtomicUsize);

impl<const LIMIT: usize> ByteBudget<LIMIT> {
    /// Reserve `bytes` if the running total would stay within `LIMIT`.
    ///
    /// `checked_add` is load-bearing, not decoration: a reservation that would
    /// overflow `usize` fails rather than wrapping the running total back down
    /// into a budget that looks free.
    pub fn reserve(&self, bytes: usize) -> bool {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(bytes).filter(|&next| next <= LIMIT)
            })
            .is_ok()
    }

    /// Give a reservation back once its payload was delivered or dropped.
    ///
    /// Release exactly what was reserved: like the two copies this replaced,
    /// releasing more panics in debug and wraps in release.
    pub fn release(&self, bytes: usize) {
        self.0.fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Bytes currently reserved.
    pub fn load(&self, ordering: Ordering) -> usize {
        self.0.load(ordering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reservation_is_refused_once_it_would_cross_the_limit() {
        let budget = ByteBudget::<10>::default();
        assert!(budget.reserve(10), "exactly the limit fits");
        assert!(!budget.reserve(1), "one byte over is refused");
        assert_eq!(
            budget.load(Ordering::Acquire),
            10,
            "a refusal reserves nothing"
        );
        budget.release(10);
        assert!(budget.reserve(10), "a release makes the room again");
    }

    /// The guard this type exists to keep: `checked_add` means a reservation
    /// that would overflow `usize` fails instead of wrapping the running total
    /// back down into a budget that looks free.
    #[test]
    fn a_reservation_that_would_overflow_fails_instead_of_wrapping() {
        let budget = ByteBudget::<{ usize::MAX }>::default();
        assert!(budget.reserve(1));
        assert!(!budget.reserve(usize::MAX), "no wrap-around");
        assert_eq!(budget.load(Ordering::Acquire), 1);
    }
}

#[cfg(test)]
mod verify_us0093 {
    //! `US-0093`'s independent verification wrote these three and they are kept:
    //! the packet's own tests cover the limit, the refusal and the `checked_add`
    //! guard single-threaded, while these pin the contended case, a refusal at
    //! the real 4 MiB ceiling, and `reserve(0)`.
    use super::*;
    use std::sync::Arc;

    /// N threads hammer reserve/release against a small `LIMIT`: the running
    /// total must never be observed above `LIMIT`, and must land back on 0.
    #[test]
    fn concurrent_reserve_and_release_never_exceed_the_limit_and_return_to_zero() {
        // LIMIT < THREADS * CHUNK, so the budget is genuinely contended and the
        // ceiling is actually reached — otherwise the assertion never fires.
        const LIMIT: usize = 20;
        const THREADS: usize = 8;
        const ROUNDS: usize = 20_000;
        const CHUNK: usize = 7;

        let budget = Arc::new(ByteBudget::<LIMIT>::default());
        let peak = Arc::new(AtomicUsize::new(0));
        let granted = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                let budget = Arc::clone(&budget);
                let peak = Arc::clone(&peak);
                let granted = Arc::clone(&granted);
                scope.spawn(move || {
                    for _ in 0..ROUNDS {
                        if budget.reserve(CHUNK) {
                            granted.fetch_add(1, Ordering::Relaxed);
                            let seen = budget.load(Ordering::Acquire);
                            peak.fetch_max(seen, Ordering::Relaxed);
                            assert!(seen <= LIMIT, "observed {seen} bytes over LIMIT {LIMIT}");
                            std::thread::yield_now();
                            budget.release(CHUNK);
                        }
                    }
                });
            }
        });

        assert_eq!(
            budget.load(Ordering::Acquire),
            0,
            "every reservation was released"
        );
        let peak = peak.load(Ordering::Relaxed);
        assert!(peak <= LIMIT, "peak {peak} exceeded LIMIT {LIMIT}");
        assert!(
            granted.load(Ordering::Relaxed) > 0,
            "the test did no work at all"
        );
    }

    /// The real 4 MiB ceiling both backends instantiate: a refusal must leave
    /// the running total untouched and the last byte must still fit.
    #[test]
    fn a_refused_reservation_leaves_the_total_untouched() {
        const LIMIT: usize = 4 * 1024 * 1024;
        let budget = ByteBudget::<LIMIT>::default();
        assert!(budget.reserve(LIMIT - 1));
        assert!(!budget.reserve(2), "two over the remaining one is refused");
        assert_eq!(budget.load(Ordering::Acquire), LIMIT - 1);
        assert!(budget.reserve(1), "the last byte still fits");
        assert_eq!(budget.load(Ordering::Acquire), LIMIT);
        budget.release(LIMIT);
        assert_eq!(budget.load(Ordering::Acquire), 0);
    }

    /// `reserve(0)` always succeeds, even at the ceiling. Both deleted copies
    /// behaved this way; pinned so a later change is a deliberate one.
    #[test]
    fn a_zero_byte_reservation_always_succeeds() {
        let budget = ByteBudget::<4>::default();
        assert!(budget.reserve(4));
        assert!(budget.reserve(0), "zero bytes fit even at the ceiling");
        assert_eq!(budget.load(Ordering::Acquire), 4);
    }
}
