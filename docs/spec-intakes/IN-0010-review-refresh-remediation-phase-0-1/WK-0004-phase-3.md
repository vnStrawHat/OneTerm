# Work: Phase 3 — polish sweep

ID: WK-0004
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

- Change type: maintenance
- Risk lane: high-risk
- Spec Intake, when required: `IN-0010`

## Outcome

The remaining Medium/Low items in `docs/review-refresh-2026-08/` files 01–08 (Phase 3 of the plan) are
fixed or explicitly deferred with a reason, crate by crate.

## Scope

- [x] In scope: SEC-03/04/07/09/10, SEC-18..SEC-25, CORR-30..CORR-70, ERR-01/02/04/06..15,
  PERF-12..17, PERF-21..31, TEST-03/04/05/13/14/15/18/20/21/24/25, HYG-02..HYG-16, HYG-18..HYG-23,
  BUILD-07..11, BUILD-14..17, BUILD-20, BUILD-22, BUILD-24, BUILD-26..28, plus the unphased ARCH items
  (ARCH-11, ARCH-36, ARCH-38, ARCH-39, ARCH-41..ARCH-44).
- [x] Out of scope: anything requiring external side effects (upstream PRs, releases, network fetches).

## Acceptance

- [ ] Every in-scope item is either ticked in files 01–08 or listed under Gaps with a reason.
- [ ] Workspace gate green on the integration branch.

## Documentation

### Owning Docs Reviewed

- Per item, as listed in the checklists; `docs/README.md` (new), `docs/agents/*.md`, `README.md`.

### Documentation Action

Update required: HYG-16/18/19/20 doc items are themselves documentation work.

### Reconciliation

Pending.

## Plan

- [ ] Wave E (parallel by crate ownership): E1 terminal + backends; E2 terminal-view; E3 update +
  settings-ui + app; E4 sftp-ui + session-ui + state + settings + theme + workspace; E5 highlight +
  completion + core; E6 docs, scripts, manifests, workflows.

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

Pending.
