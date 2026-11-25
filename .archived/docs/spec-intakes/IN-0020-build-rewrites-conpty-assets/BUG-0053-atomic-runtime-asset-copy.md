# Work: Copy runtime assets only when changed and atomically

ID: BUG-0053
Intake: IN-0020
Created: 2026-09-09

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

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0020

## Outcome

`cargo build` leaves `conpty.dll` and `x64/OpenConsole.exe` untouched when they already match
the assets, and replaces them atomically otherwise.

## Scope

- [x] In scope: `copy_runtime_asset` in `crates/app/build.rs`.
- [x] Out of scope: release packaging scripts.

## Acceptance

- [x] A second `cargo build -p oneterm-app` does not change the asset modification times.
- [x] A changed asset is replaced via `.tmp` + rename; no `.tmp` is left behind on failure.
- [ ] `pwsh scripts/ci-local.ps1` green (run with the next gate).

## Documentation

### Owning Docs Reviewed

- `crates/app/build.rs` header (layout comment) — unchanged.
- `docs/agents/structure.md` — build.rs description unchanged.

### Documentation Action

No contract change: the layout and file names are the same; only the copy mechanics changed.

Reason: developer-facing behaviour only.

### Reconciliation

No docs changed besides this packet and its intake.

## Context

The owner hit 0xc0000142 while an agent was rebuilding `target/fast-dev` repeatedly; the same
binary launches cleanly when no build runs.

## Plan

- [x] Skip unchanged assets (size + mtime).
- [x] Copy through a `.tmp` sibling and rename.

## Decisions

None.

## Verification Plan

- `cargo build -p oneterm-app` twice; compare the asset modification times.
- `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

`cargo build -p oneterm-app` twice (2026-09-09): `target/debug/x64/OpenConsole.exe` and `conpty.dll` modification times identical across the rebuild, no `*.tmp` left; `rustfmt --check` clean. Full gate runs with the next ci-local pass.

## Handoff

None.
