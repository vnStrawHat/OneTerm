# Work: Phase 0 — stop the bleeding

ID: WK-0001
Intake: IN-0010
Created: 2026-08-17

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: high-risk
- Spec Intake, when required: `IN-0010`

## Outcome

All Phase 0 items in `docs/review-refresh-2026-08/09-remediation-plan.md` are fixed with regression tests.

## Scope

- [x] In scope: CORR-01, SEC-01, SEC-02, CORR-07, CORR-02, CORR-03, CORR-05, CORR-06, CORR-04, CORR-12,
  CORR-08, CORR-23, ARCH-12 (stop-gap), SEC-11, BUILD-01, BUILD-23; tests TEST-06..TEST-12.
- [x] Out of scope: structural refactors (Phase 2), remaining Medium/Low items (Phase 3).

## Acceptance

- [x] Each item's regression test exists and passes.
- [x] Workspace gates green on the integration branch (fmt, clippy -D warnings, 632 tests, build, 4 Python checks; 2026-08-17).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md`, `docs/agents/error-policy.md`, `docs/agents/persistence.md`,
  `docs/auto-update.md`, `docs/ssh-authentication.md`.

### Documentation Action

Update required: `docs/terminal-backend.md` (event delivery no longer blocks under the Term lock).

### Reconciliation

`docs/terminal-backend.md` §5.3/§6.5/§7 updated (event delivery never blocks under the Term lock). No other owning contract changed.

## Plan

- [x] Groups: A1 backends, A2 terminal+completion, A3 terminal-view, A4 workspace/app/settings, A5 update, A6 CI.

## Verification Plan

Per-crate `cargo test -p`, then full workspace gate on the merged branch.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof (Windows local; Linux/macOS via CI)
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Commits: dec800f, 74838d2, fff31f3, a4e17af, 76ae76d, 42d9a51, 3986790, 73193ce, c2960f1, 8f855e7, b76a881, 5f75ab7, d5d2ca1 (merged into `fix/review-remediation-phase-0-1`).
- Gaps: CORR-04 has no unit test (no injectable path seam for docks.json in `workspace`); ARCH-12 is the stop-gap only (proper `RemotePath` in WK-0002); Linux/macOS runs of the new updater tests happen only in CI.
