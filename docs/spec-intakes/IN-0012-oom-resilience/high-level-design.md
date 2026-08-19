# High-Level Design: OOM resilience

Intake: IN-0012

## Problem

Windows has no OOM killer. When system commit charge is exhausted (typically a
`rustc`/`cargo` spawned by a coding agent inside a OneTerm tab), every
allocating process gets NULL from the OS. Rust's infallible-allocation path
responds by aborting through `__fastfail` (exception `0xc0000409`), which
bypasses both the Rust panic hook and the `crash-handler` native callback —
OneTerm dies instantly with an empty crash report. OneTerm allocates
continuously while a tab is producing output (PTY pump, per-frame render
snapshot), so it almost always lands inside the exhaustion window.

The exhaustion is almost always a short spike: the offending process hits the
same NULL, aborts, and frees gigabytes within milliseconds. OneTerm only dies
because it has no chance to retry.

## Design

Two independent layers:

### Layer 1 — OOM-resilient global allocator (`crates/app/src/oom.rs`)

`#[global_allocator]` wrapper around `std::alloc::System`:

1. At startup, commit a 64 MiB **ballast** block.
2. On the first failed allocation, free the ballast (instant commit headroom)
   and retry the allocation on a 20 ms sleep loop, up to ~3 s total.
3. Only when memory is still exhausted after that, return NULL and let Rust
   abort as before.

Success path cost: one null check. Deliberate trade-offs (freeze-for-crash,
one-shot ballast, no platform gate) are documented in the module header and in
`DEC-0005`.

### Layer 2 — allocation-free render snapshot (P1)

`TerminalContent::from()` previously built a fresh `Vec<IndexedCell>`
(~rows×cols ≈ hundreds of KB) plus a dirty-line `Vec` **every frame**. Now:

- `TerminalContent::refill(&mut self, term)` refills a buffer in place,
  retaining `cells` and dirty-line capacity (`from()` remains as an
  allocating convenience for tests/queries).
- `TerminalModel::snapshot_into` / `TerminalRender::snapshot_into` (defaulted
  to `*out = self.snapshot()` so `FakeTerminalSession` and other simple
  implementors are unaffected) thread the reusable buffer through.
- The buffer lives in `RenderCache.snapshot`; prepaint holds one
  `RefCell` borrow across snapshot → row cache → metrics (disjoint field
  borrows).

Steady-state render loop: zero allocation in the snapshot path.

## Out of scope

- Job-object memory caps for child shells (separate opt-in feature if ever
  requested).
- Protection for non-Rust allocations (GPU/driver, thread stacks, C libs).
- Re-committing the ballast after recovery.

## UI Wireframe

N/A — no user-facing UI change.
