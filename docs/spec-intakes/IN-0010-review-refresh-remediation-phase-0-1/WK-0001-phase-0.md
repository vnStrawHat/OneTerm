# Work: Phase 0 — stop the bleeding

ID: WK-0001
Intake: IN-0010
Created: 2026-08-17

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
- Risk lane: high-risk
- Spec Intake, when required: `IN-0010`

## Outcome

All Phase 0 items in `docs/review-refresh-2026-08/09-remediation-plan.md` are fixed with regression tests.

## Scope

- [x] In scope: CORR-01, SEC-01, SEC-02, CORR-07, CORR-02, CORR-03, CORR-05, CORR-06, CORR-04, CORR-12,
  CORR-08, CORR-23, ARCH-12 (stop-gap), SEC-11, BUILD-01, BUILD-23; tests TEST-06..TEST-12.
- [x] Out of scope: structural refactors (Phase 2), remaining Medium/Low items (Phase 3).

## Acceptance

- [ ] Each item's regression test exists and passes.
- [ ] Workspace gates green on the integration branch.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md`, `docs/agents/error-policy.md`, `docs/agents/persistence.md`,
  `docs/auto-update.md`, `docs/ssh-authentication.md`.

### Documentation Action

Update required: `docs/terminal-backend.md` (event delivery no longer blocks under the Term lock).

### Reconciliation

To be filled at completion.

## Plan

- [x] Groups: A1 backends, A2 terminal+completion, A3 terminal-view, A4 workspace/app/settings, A5 update, A6 CI.

## Verification Plan

Per-crate `cargo test -p`, then full workspace gate on the merged branch.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

To be filled at completion.
