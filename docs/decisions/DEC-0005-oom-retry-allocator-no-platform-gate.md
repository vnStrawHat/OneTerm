# DEC-0005 OOM handling: retry allocator with ballast, not platform-gated

Date: 2026-08-19

## Status

Accepted

## Context

System-wide memory exhaustion (agent-driven `cargo` builds) killed OneTerm via
Rust's abort-on-failed-allocation (`__fastfail`, `0xc0000409`) before any
crash capture could run. Candidate mitigations: an OOM-retrying global
allocator, preallocating scrollback per terminal, job-object memory caps on
child shells, and `#[cfg(windows)]`-gating any of the above.

## Decision

1. OneTerm wraps the system allocator (`crates/app/src/oom.rs`): on failure,
   release a 64 MiB startup ballast and retry for ~3 s before aborting.
   Future code must not assume a failed allocation aborts instantly, and must
   never allocate on the allocator's failure path.
2. The wrapper is **not** platform-gated. macOS also lacks an OOM killer
   (`malloc` returns NULL when swap is exhausted); on Linux the wrapper is
   inert but harmless. One code path on all platforms, exercised by every CI
   target.
3. **Trade freeze for crash**: during a spike the UI may stall up to ~3 s
   (sleep inside the allocator, possibly under user locks). Accepted — a
   short freeze that can end in survival beats an instant report-less abort.
4. Hot paths minimize continuous allocation by **buffer reuse** (render
   snapshot refilled in place), never by preallocating worst-case capacity
   up front — preallocation raises baseline commit and makes exhaustion more
   likely, the opposite of the goal.

## Alternatives

- [x] Selected approach described above.
- [ ] `#[cfg(windows)]` gate — drops real macOS protection, splits behavior
  across platforms, removes the code from non-Windows CI; saves nothing
  measurable.
- [ ] Preallocate scrollback (10k lines) per terminal — pays ~50–65 MB commit
  per tab up front, does not cover the other allocation sites, and raises
  baseline pressure (see 4).
- [ ] Job-object memory caps on child shells — strongest prevention but
  constrains user workloads; deferred as a possible opt-in setting, not a
  default.

## Consequences

- [ ] Benefit to confirm: OneTerm survives the next real agent-driven OOM
  spike (manual observation).
- [x] Tradeoff: ballast is one-shot per process lifetime; later spikes rely on
  the retry loop alone. Revisit re-commit only with evidence of multi-spike
  sessions dying.
