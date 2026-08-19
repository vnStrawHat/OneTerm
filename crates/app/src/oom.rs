//! OOM resilience — retry failed allocations instead of aborting immediately.
//!
//! Windows has no OOM killer: when system commit charge is exhausted, *every*
//! allocating process gets NULL back, and Rust's infallible allocation path
//! (`Vec::push`, `String`, …) responds by aborting via `__fastfail`
//! (exception `0xc0000409`) — skipping both the panic hook and the native
//! crash handler, so OneTerm dies with an empty crash report.
//!
//! Exhaustion is almost always a short spike: the offending process (e.g.
//! `rustc` spawned by a coding agent) hits the same NULL, aborts, and frees
//! gigabytes within milliseconds. This allocator turns OneTerm's "abort on
//! first NULL" into "release a preallocated ballast, then wait out the spike":
//!
//! 1. On startup [`init_ballast`] commits a ballast block, adding headroom to
//!    OneTerm's commit charge while memory is plentiful.
//! 2. When an allocation fails, the ballast is freed (instant headroom) and
//!    the allocation is retried on a sleep loop for a few seconds.
//! 3. Only if memory is still exhausted after that does the allocator give up
//!    and let Rust abort as before.
//!
//! Best effort by design — known, deliberate trade-offs:
//!
//! - **Trade freeze for crash.** The retry loop sleeps *inside* the allocator,
//!   possibly while the calling thread holds arbitrary locks — other threads
//!   blocked on those locks stall too, so the UI can freeze for up to ~3 s
//!   during an OOM spike. If the pressure comes from OneTerm itself (not a
//!   sibling process), the retry cannot succeed and only delays the abort by
//!   those 3 s. Accepted: a short freeze that can end in survival beats an
//!   instant, report-less abort.
//! - **The ballast is one-shot.** Once released it is never re-committed, so
//!   later spikes in the same session rely on the retry loop alone (still
//!   effective: the dying sibling frees its memory within the window).
//!   Re-committing after recovery would need a "system is stable again"
//!   heuristic — not worth the complexity until proven necessary.
//! - Allocations made *outside* the Rust allocator (GPU/driver, thread
//!   stacks, C libraries) are not protected.
//! - Not `#[cfg(windows)]`-gated: macOS has no OOM killer either (`malloc`
//!   returns NULL when swap is exhausted), and on Linux the wrapper is inert
//!   but harmless — the untouched ballast is lazily committed and the success
//!   path is a single null check.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Duration;

/// Ballast size (64 MiB). Committed at startup, freed on the first failed
/// allocation to give the retry loop immediate headroom.
const BALLAST_SIZE: usize = 64 * 1024 * 1024;
const BALLAST_ALIGN: usize = 8;

/// Retry cadence: 150 × 20 ms ≈ 3 s total. An OOM spike caused by a dying
/// sibling process resolves well within this window.
const RETRY_DELAY: Duration = Duration::from_millis(20);
const MAX_RETRIES: u32 = 150;

/// The committed ballast block; null once released (or never committed).
static BALLAST: AtomicPtr<u8> = AtomicPtr::new(ptr::null_mut());

fn ballast_layout() -> Layout {
    // Both constants are non-zero powers-of-two-compatible values.
    Layout::from_size_align(BALLAST_SIZE, BALLAST_ALIGN).expect("valid const ballast layout")
}

/// Commit the ballast block. Call once at startup; failure (or a second call)
/// is silently ignored — the allocator degrades to plain retry.
pub(crate) fn init_ballast() {
    let layout = ballast_layout();
    // SAFETY: `layout` has non-zero size.
    let block = unsafe { System.alloc(layout) };
    if block.is_null() {
        return;
    }
    if BALLAST
        .compare_exchange(ptr::null_mut(), block, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        // Already committed by an earlier call — release the extra block.
        // SAFETY: `block` was just allocated with `layout` and never published.
        unsafe { System.dealloc(block, layout) };
    }
}

/// Free the ballast if still held. Returns whether headroom was released.
fn release_ballast() -> bool {
    let block = BALLAST.swap(ptr::null_mut(), Ordering::AcqRel);
    if block.is_null() {
        return false;
    }
    // SAFETY: a non-null `BALLAST` was allocated by `System.alloc` with
    // `ballast_layout()` and the swap guarantees exactly one thread frees it.
    unsafe { System.dealloc(block, ballast_layout()) };
    true
}

/// Global allocator wrapping [`System`]: identical on the success path, and on
/// failure releases the ballast and retries before letting Rust abort.
pub(crate) struct OomResilientAlloc;

impl OomResilientAlloc {
    /// Retry `attempt` on a sleep loop after releasing the ballast.
    ///
    /// Must not allocate: it runs while the heap is exhausted. `release_ballast`
    /// only frees, and `std::thread::sleep` does not allocate.
    #[cold]
    fn retry(attempt: impl Fn() -> *mut u8) -> *mut u8 {
        release_ballast();
        for _ in 0..MAX_RETRIES {
            let block = attempt();
            if !block.is_null() {
                return block;
            }
            std::thread::sleep(RETRY_DELAY);
        }
        ptr::null_mut()
    }
}

// SAFETY: pure delegation to `System` (which upholds the `GlobalAlloc`
// contract); the retry path re-invokes the same `System` primitive with the
// same arguments, so every returned pointer originates from `System` with the
// layout the caller passed.
unsafe impl GlobalAlloc for OomResilientAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract (non-zero-sized `layout`).
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            return block;
        }
        // SAFETY: same contract as above on each retry.
        Self::retry(|| unsafe { System.alloc(layout) })
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded caller contract (non-zero-sized `layout`).
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            return block;
        }
        // SAFETY: same contract as above on each retry.
        Self::retry(|| unsafe { System.alloc_zeroed(layout) })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded caller contract (`ptr` came from this allocator,
        // which only ever returns `System` pointers, with the same `layout`).
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded caller contract (`ptr`/`layout` valid for this
        // allocator, `new_size` non-zero and layout-compatible).
        let block = unsafe { System.realloc(ptr, layout, new_size) };
        if !block.is_null() {
            return block;
        }
        // SAFETY: `System.realloc` leaves the original block valid on failure,
        // so retrying with the same arguments remains within the contract.
        Self::retry(|| unsafe { System.realloc(ptr, layout, new_size) })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    /// Serialize tests: they share the process-global `BALLAST`.
    fn serial() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn retry_recovers_after_transient_failure_and_releases_ballast() {
        let _guard = serial();
        init_ballast();

        let attempts = AtomicU32::new(0);
        let layout = Layout::from_size_align(64, 8).expect("valid test layout");
        let block = OomResilientAlloc::retry(|| {
            // The ballast must already be released when a retry attempt runs.
            assert!(BALLAST.load(Ordering::Acquire).is_null());
            if attempts.fetch_add(1, Ordering::Relaxed) < 2 {
                ptr::null_mut()
            } else {
                // SAFETY: `layout` has non-zero size.
                unsafe { System.alloc(layout) }
            }
        });

        // Two transient failures, then success on the third attempt.
        assert!(!block.is_null());
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
        // The failure path already released the ballast.
        assert!(!release_ballast());
        // SAFETY: `block` was allocated above with `layout`.
        unsafe { System.dealloc(block, layout) };
    }

    #[test]
    fn ballast_lifecycle_and_normal_alloc() {
        let _guard = serial();
        init_ballast();
        // Double init must not leak or replace the committed block.
        let first = BALLAST.load(Ordering::Acquire);
        init_ballast();
        assert_eq!(BALLAST.load(Ordering::Acquire), first);

        // Normal allocations flow through the wrapper unchanged.
        let data: Vec<u64> = (0..1024).collect();
        assert_eq!(data.len(), 1024);

        // Release frees exactly once.
        assert!(release_ballast());
        assert!(!release_ballast());
    }
}
