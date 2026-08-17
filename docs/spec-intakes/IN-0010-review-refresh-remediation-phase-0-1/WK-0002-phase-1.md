# Work: Phase 1 — contracts and CI identity

ID: WK-0002
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

All Phase 1 items in `docs/review-refresh-2026-08/09-remediation-plan.md` are fixed with regression tests.

## Scope

- [x] In scope: BUILD-02, BUILD-13, BUILD-03, BUILD-04, BUILD-05, BUILD-06, BUILD-12, BUILD-18, BUILD-19, BUILD-25, ARCH-12, ARCH-05, ARCH-04, ARCH-03, ARCH-08, ERR-03, ARCH-07, SEC-05, SEC-06, SEC-13, SEC-14, CORR-09, CORR-14, CORR-15, ARCH-37, CORR-16, ERR-05, CORR-25..CORR-29, HYG-17; tests TEST-22, TEST-23.

- [x] Out of scope: structural refactors (Phase 2), remaining Medium/Low items (Phase 3).

## Acceptance

- [ ] Each item's regression test exists and passes.
- [ ] Workspace gates green on the integration branch.

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md`, `docs/agents/error-policy.md`, `docs/agents/persistence.md`,
  `docs/auto-update.md`, `docs/ssh-authentication.md`.

### Documentation Action

Update required: `docs/agents/dependencies.md` (vendoring), `docs/sftp-browser-design.md` (RemotePath, TransferHandle), `AGENTS.md` (full CI gate).

### Reconciliation

To be filled at completion.

## Plan

- [x] Groups: B1 cargo/build/CI identity, B2 SFTP contract, B3 SSH/local security + take_events, B4 panel names, B5 single-writer persistence, B6 encoding fixes.

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
