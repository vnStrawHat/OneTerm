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
