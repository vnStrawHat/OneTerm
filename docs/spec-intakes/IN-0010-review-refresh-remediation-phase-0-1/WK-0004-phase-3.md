# Work: Phase 3 — polish sweep

ID: WK-0004
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

- [x] Every in-scope item is either ticked in files 01–08 or listed under Gaps with a reason (see
  `09-remediation-plan.md` "Open items after Phase 3").
- [x] Workspace gate green on the integration branch (922 tests, 2026-08-18).

## Documentation

### Owning Docs Reviewed

- Per item, as listed in the checklists; `docs/README.md` (new), `docs/agents/*.md`, `README.md`.

### Documentation Action

Update required: HYG-16/18/19/20 doc items are themselves documentation work.

### Reconciliation

Updated: `docs/README.md` (new index), `docs/PROJECT.md`, `README.md`, `docs/archive/**` (July reviews moved),
`docs/auto-update.md` (hardening, wired knobs, signature decision), `docs/crash-reporting.md`,
`docs/agents/dependencies.md`, `docs/agents/ui-fork-maintenance.md` (upstreaming), `scripts/README.md`,
`NOTICE`, `THIRD-PARTY-NOTICES.md` (generated), `.editorconfig`, `docs/auto-completion/08-security-redaction.md`,
`docs/terminal-semantic-highlighting.md`, `docs/agents/persistence.md` (load outcomes), `docs/gui-layout.md`.

## Plan

- [x] Wave D/F (parallel by crate ownership): E1 terminal + backends; E2 terminal-view; E3 update +
  settings-ui + app; E4 sftp-ui + session-ui + state + settings + theme + workspace; E5 highlight +
  completion + core; E6 docs, scripts, manifests, workflows.

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

- Same merge set and gate as WK-0003 (Phase 2 and Phase 3 were executed by the same crate-owned waves).
- Gaps: BUILD-07 (duplicate crate versions pinned upstream), BUILD-21 (external), ERR-06 / HYG-04 / HYG-11 /
  HYG-12 / HYG-14 partial sweeps, TEST-05 partial, CORR-67 partial; the SEC "Info" line is informational.
