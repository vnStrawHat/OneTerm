# Work: OOM ballast is 16 MiB

ID: US-0141
Intake: IN-0045
Created: 2026-09-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change
- Risk lane: normal (one constant in the app allocator; no persisted data, no public
  contract)
- Spec Intake, when required: [`IN-0045`](IN-0045.md)
- Contract: [`DEC-0020`](../../decisions/DEC-0020-oom-ballast-size.md), accepted by the
  owner 2026-09-25 at 16 MiB.

## Outcome

The OOM ballast committed at startup is 16 MiB instead of 64 MiB. Every instance charges
48 MB less commit from startup; the private working set does not change. The sizing rule
is written in the `oom.rs` module header, so the number is re-derived rather than copied.

## Scope

- [x] In scope:
  - `crates/app/src/oom.rs`: `BALLAST_SIZE` 64 → 16 MiB; the sizing rule, with the measured
    numbers and the `DEC-0020` citation, in the module header.
  - Docs that state "64 MiB" for the ballast.
- [x] Out of scope:
  - The retry loop (20 ms × 150) and the one-shot release: unchanged (`DEC-0020`).
  - `BUG-0078` (the 24.8 MB glyph table). It is on its own branch; see Handoff for the
    merge order.

## Acceptance

- [x] `BALLAST_SIZE` is `16 * 1024 * 1024`; `RETRY_DELAY`, `MAX_RETRIES` and the release
  path are unchanged.
- [x] The `oom.rs` module header states the rule from `DEC-0020`, the measured inputs, and
  cites `DEC-0020`.
- [x] Existing `oom` tests pass.
- [x] Release build, `measure.ps1` S1, 2 runs: commit about 48 MB lower than main, private
  working set unchanged within noise.
- [x] `vmregions.ps1` on the new build shows a 16.0 MB committed block with 0 resident and
  no 64.0 MB block.
- [x] No doc still states a 64 MiB ballast as current.

## Documentation

### Owning Docs Reviewed

- `docs/decisions/DEC-0020-oom-ballast-size.md`: the contract; lists what must land.
- `docs/decisions/DEC-0005-oom-retry-allocator-no-platform-gate.md`: "release a 64 MiB
  startup ballast". Update required.
- `docs/spec-intakes/IN-0012-oom-resilience/low-level-design/oom-allocator.md`: the
  `BALLAST_SIZE` row says 64 MiB. Update required.
- `docs/spec-intakes/IN-0012-oom-resilience/high-level-design.md`: "commit a 64 MiB
  ballast". Update required.
- `docs/crash-reporting.md`: "releases a 64 MiB startup ballast". Update required.
- `docs/spec-intakes/IN-0012-oom-resilience/BUG-0012-survive-system-oom-spikes.md`: a
  shipped packet's record; it states no size. No change.
- `docs/spec-intakes/IN-0045-memory-usage-review/IN-0045.md` and `high-level-design.md`:
  the `DEC-0020` line and the budget row say "proposed" and 64. Update required.
- `crates/app/src/lib.rs`: a comment says "a 64 MiB allocation". Update required.
- `research/*.md` under `IN-0045`: measurements of the 64 MiB build at the time. History,
  no change.

### Documentation Action

- Update required: the files marked above.

Reason: the ballast size is stated in several owning docs; they must match the code.

### Reconciliation

Changed: `crates/app/src/oom.rs` (module header: the sizing rule and its inputs;
`BALLAST_SIZE` doc), the `crates/app/src/lib.rs` comment, `DEC-0005` Decision 1,
`IN-0012` `high-level-design.md` and `low-level-design/oom-allocator.md`,
`docs/crash-reporting.md`, `IN-0045.md` (the `DEC-0020` lines and owner question), and the
`IN-0045` `high-level-design.md` diagram and budget row. No test asserts the size, so no
test changed.

## Context

- The ballast was introduced by `BUG-0012` (commit `05df6ffa`).
- `DEC-0020` sizes the ballast on the assumption that `BUG-0078` has removed the 24.8 MB
  glyph-table allocation. Without it the decision says 32 MiB.

## Plan

- [x] Packet (this file).
- [x] `oom.rs` constant and header; `lib.rs` comment.
- [x] Docs.
- [x] Measure main and this branch (release, S1, 2 runs each; `vmregions.ps1`).
- [x] Gates, commit.

## Decisions

- [`DEC-0020`](../../decisions/DEC-0020-oom-ballast-size.md).

## Verification Plan

- `cargo test -p oneterm-app` (the two `oom` tests).
- `measure.ps1 -Mode S1` on a release build of main and of this branch, 2 runs each,
  private `USERPROFILE`, own pid only; `vmregions.ps1 -ProcessId <own pid>` with `-Hold`.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `python scripts/check-doc-paths.py`, `python scripts/check-english.py`, then the full
  `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Unit: `cargo test -p oneterm-app`: 26 passed, including
  `oom::tests::ballast_lifecycle_and_normal_alloc` and
  `oom::tests::retry_recovers_after_transient_failure_and_releases_ballast`.
- Platform (Windows 11 26200, release build, `measure.ps1 -Mode S1`, private
  `USERPROFILE` created by the script, own pid only, MB = 2^20 B):

  | Build | Run | WS | Private WS | Commit |
  | --- | --- | --- | --- | --- |
  | main `5351f09a` | 1 | 126.9 | 49.2 | 182.1 |
  | main `5351f09a` | 2 | 93.4 | 49.0 | 181.8 |
  | this branch | 1 | 94.4 | 49.5 | 134.2 |
  | this branch | 2 | 93.2 | 48.8 | 133.5 |

  Commit: 182.0 → 133.9 mean, −48.1 MB (expected 182 − 48 = 134). Private WS: 49.1 →
  49.2, unchanged within noise. The first main run's WS is a startup outlier; the steady
  runs agree.
- `vmregions.ps1` on a held S1 instance of this branch (pid 32476): one 16.0 MB committed
  allocation with 0.0 MB resident; no allocation of 32 MB or more. The 23.6 MB
  0-resident block next to it is the glyph table (`BUG-0078`).
- Gates: `cargo fmt --all -- --check`, `python scripts/check-doc-paths.py`,
  `python scripts/check-english.py` passed; full `pwsh scripts/ci-local.ps1` with
  `CARGO_BUILD_JOBS=4`: `ci-local: all checks passed.` The run used the shared `CARGO_TARGET_DIR` through a temporary junction from the worktree's `target` (for `vt-public-api.py`), removed afterwards.
- Integration and E2E proof: not applicable to a constant. The `BUG-0012` acceptance item
  "OneTerm survives an agent-driven OOM spike" is still unobserved at any ballast size
  (`DEC-0020` Consequences).
- One fat-LTO release link of this branch died with `0xc0000409` while the machine's
  commit was near its limit (8.3 GB free of 31.7 GB); the rebuild succeeded. That is the
  sibling-process OOM this allocator is for, hitting `rustc`, not OneTerm.

## Handoff

- Merge order: `DEC-0020` sizes 16 MiB on the assumption that `BUG-0078` (branch
  `fix/glyph-cache-prealloc`, not on main when this was written) has removed the 24.8 MB
  glyph-table allocation. Merge `BUG-0078` first or together with this packet. If
  `BUG-0078` slips, the decision says 32 MiB.
