# Low-Level Design: OOM-resilient allocator

Intake: IN-0012 · Concern: `crates/app/src/oom.rs`

## Constants

| Name | Value | Rationale |
| --- | --- | --- |
| `BALLAST_SIZE` | 64 MiB | Enough headroom to finish in-flight allocations and paint; small enough to be invisible on a machine that can run OneTerm. |
| `RETRY_DELAY` / `MAX_RETRIES` | 20 ms × 150 ≈ 3 s | A dying sibling process (rustc OOM-abort) frees its memory well within this window. |

## Invariants

1. **No allocation on the failure path.** `retry()` runs while the heap is
   exhausted: it only frees (`release_ballast`) and sleeps
   (`std::thread::sleep` does not allocate). Anything else risks recursion
   into the failing allocator.
2. **Ballast is freed exactly once.** `BALLAST` is an `AtomicPtr`; `swap` to
   null decides a single winner. `init_ballast` uses `compare_exchange` so a
   double call frees the extra block instead of leaking or replacing.
3. **Success path is unchanged.** One null check, then the `System` pointer is
   returned as-is; `dealloc` is pure delegation, so every pointer this
   allocator hands out is a `System` pointer with the caller's layout.
4. **`realloc` retry is contract-safe.** `System.realloc` leaves the original
   block valid on failure, so re-invoking with identical arguments is legal.

## Failure behavior

Retries exhausted → return null → Rust `handle_alloc_error` → abort, exactly
as without this module. The module widens the survival window; it does not
change the final failure mode.

## Testing

Real OOM cannot be simulated safely in a unit test. `retry()` takes the
attempt as a closure, so tests inject failures directly:

- `retry_recovers_after_transient_failure_and_releases_ballast` — two NULL
  attempts then success; asserts the ballast is already released when the
  first retry attempt runs, exactly 3 attempts, and no double release.
- `ballast_lifecycle_and_normal_alloc` — double-init safety and single
  release.
- Tests share the process-global `BALLAST` and are serialized with a local
  mutex.
- Deliberately untested: the retries-exhausted path (a trivial loop
  exhaustion that would cost a real 3 s sleep per run).
