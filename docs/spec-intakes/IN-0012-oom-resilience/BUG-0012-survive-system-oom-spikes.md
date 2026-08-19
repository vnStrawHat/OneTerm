# Work: Survive system-OOM spikes; allocation-free render snapshot

ID: BUG-0012
Intake: IN-0012
Created: 2026-08-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: high_risk (global allocator affects broad established behavior)
- Spec Intake, when required: IN-0012

## Outcome

A system-wide OOM spike caused by a sibling process (agent-driven `cargo`
build) no longer kills OneTerm instantly with exception `0xc0000409` and an
empty crash report: allocations retry for ~3 s after releasing a ballast, long
enough for the dying process to free its memory. Independently of the spike,
the steady-state render loop allocates nothing in the snapshot path.

## Scope

- [x] In scope: `OomResilientAlloc` + ballast in `crates/app`;
  `TerminalContent::refill` / `snapshot_into` reuse in `oneterm-terminal` and
  `oneterm-terminal-view`; owning-doc updates.
- [x] Out of scope: job-object memory caps for child shells; ballast
  re-commit; non-Rust allocation (GPU, thread stacks); scrollback
  preallocation (rejected — see conversation record in IN-0012 Source).

## Acceptance

- [x] Allocation failure triggers ballast release then retry; success path is
  a single null check (code-reviewed, focused tests).
- [x] Ballast is committed at startup and released at most once.
- [x] Render prepaint reuses one `TerminalContent` buffer per view; no
  per-frame `Vec<IndexedCell>` allocation in steady state.
- [x] `TerminalRender` implementors outside the PTY macro (test fakes) compile
  unchanged via the defaulted `snapshot_into`.
- [ ] User-observed: OneTerm survives an agent-driven `cargo` OOM spike that
  previously killed it (manual, opportunistic — cannot be automated).

## Documentation

### Owning Docs Reviewed

- `docs/crash-reporting.md` — capture boundary: documents what panic/native
  capture covers; OOM fast-fail was an undocumented blind spot.
- `docs/terminal-backend.md` §2 — output data flow names
  `session.snapshot()` copying `TerminalContent` per frame.
- `docs/agents/crate-dependency-rules.md` — allocator placed in `crates/app`
  (composition root), no new dependencies.

### Documentation Action

Update required:

- `docs/crash-reporting.md` — add the OOM fast-fail limitation and the
  allocator mitigation to the capture boundary.
- `docs/terminal-backend.md` — data flow now refills a reused snapshot buffer
  (`snapshot_into`) instead of copying a fresh one.

### Reconciliation

Both docs updated in this change set (see Evidence).

## Context

- Windows event log: `0xc0000409` fast-fail, no `.crash.txt`, empty
  `.native.tmp` → abort happened inside the allocator, not a panic/SEH path.
- `[profile.release] panic = "unwind"` — a normal panic would have produced a
  report; its absence pinpoints `handle_alloc_error`.

## Plan

- [x] `crates/app/src/oom.rs`: allocator + ballast + focused tests.
- [x] `crates/app/src/lib.rs`: `#[global_allocator]`, `init_ballast()` at start of `run()`.
- [x] `TerminalContent::{default, refill}`; `from()` delegates to `refill`.
- [x] `TerminalModel::snapshot_into`; `TerminalRender::snapshot_into`
  (defaulted); PTY macro override.
- [x] `RenderCache.snapshot` buffer; prepaint single-borrow restructure.
- [x] Remove `snapshot_query`/`from_query` (allocation sweep follow-up): no
  production callers remained after `query_state`/`query_line_range_cells`;
  the damage-free full-grid clone stays deleted so an O(rows×cols)-per-event
  read cannot be reintroduced by accident.
- [x] Owning-doc updates + decision record DEC-0005.

## Decisions

- `docs/decisions/DEC-0005-oom-retry-allocator-no-platform-gate.md`

## Verification Plan

- Focused: `cargo test -p oneterm-app oom` — retry recovery + ballast lifecycle.
- Regression: `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-app oom` — 2 passed (retry recovery, ballast lifecycle).
- Allocation sweep after P1: gutter-entry strings are the only remaining
  per-frame allocation (~50 small strings/frame; shaping already cached) —
  accepted; pump/queries/paint already bounded or event-driven.
- `cargo test --workspace -j 4` — 938 passed, 0 failed (`-j 4`: an unrestricted
  parallel build OOM-killed rustc on the dev machine — the very failure mode
  this bug fixes).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- Gap: real-OOM survival is not automatable; awaiting opportunistic manual
  confirmation (last Acceptance item).
- Gap: retries-exhausted path untested by design (3 s of real sleep per run;
  see LLD).
- Gap: Harness CLI unavailable in the authoring session — status/proof blocks
  maintained manually; `harness.db` not synchronized.

## Handoff

Working tree on branch `fix/oom-resilience`, not yet committed — user is
reviewing. Next: user review → commit → manual OOM observation when the
scenario recurs.
