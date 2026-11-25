# Work: Phase 1 — contracts and CI identity

ID: WK-0002
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

All Phase 1 items in `docs/review-refresh-2026-08/09-remediation-plan.md` are fixed with regression tests.

## Scope

- [x] In scope: BUILD-02, BUILD-13, BUILD-03, BUILD-04, BUILD-05, BUILD-06, BUILD-12, BUILD-18, BUILD-19, BUILD-25, ARCH-12, ARCH-05, ARCH-04, ARCH-03, ARCH-08, ERR-03, ARCH-07, SEC-05, SEC-06, SEC-13, SEC-14, CORR-09, CORR-14, CORR-15, ARCH-37, CORR-16, ERR-05, CORR-25..CORR-29, HYG-17; tests TEST-22, TEST-23.

- [x] Out of scope: structural refactors (Phase 2), remaining Medium/Low items (Phase 3).

## Acceptance

- [x] Each item's regression test exists and passes.
- [x] Workspace gates green on the integration branch (fmt, clippy -D warnings, 687 tests, build, 5 Python checks, cargo deny; 2026-08-17).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md`, `docs/agents/error-policy.md`, `docs/agents/persistence.md`,
  `docs/auto-update.md`, `docs/ssh-authentication.md`.

### Documentation Action

Update required: `docs/agents/dependencies.md` (vendoring), `docs/sftp-browser-design.md` (RemotePath, TransferHandle), `AGENTS.md` (full CI gate).

### Reconciliation

Updated: `docs/agents/dependencies.md` (vendoring rows + policy, allowed set), `docs/agents/structure.md` §3, `docs/agents/ui-fork-maintenance.md`, `vendor/README.md`, `AGENTS.md` §4 (full CI gate + `scripts/ci-local.*`), `docs/sftp-browser-design.md` (RemotePath / TransferHandle / remove_dir_all / generation guard), `docs/agents/persistence.md` (owner-only quarantine, update_config writers), `docs/auto-update.md` (writer table), `docs/ssh-client-connect.md` §9.3, `docs/ssh-authentication.md`, `docs/architecture.md` + `docs/gui-layout.md` (panel_names), `docs/terminal-backend.md` (take_events).

## Plan

- [x] Groups: B1 cargo/build/CI identity, B2 SFTP contract, B3 SSH/local security + take_events, B4 panel names, B5 single-writer persistence, B6 encoding fixes.

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

- Commits: d9dcd66 (panel names), a252f36 c5a3645 24300c6 5238ad7 (single-writer persistence), f8e54f7 7b13477 f98a4f1 319f392 e56a7a4 (encoding), ab0c23d 247c073 b2b3772 d228d51 db43c54 (ssh/local security, take_events), 6549514 f6415cf d2a18c2 fb04106 3df694e (cargo/build/CI), a162cea f7dadcb 4e732fa (SFTP contract) — merged into `fix/review-remediation-phase-0-1`.
- Gaps / follow-ups: `windows-sys` stays 0.59 (unifying to 0.61 needs `crates/app/src/lib.rs` `BOOL` import change — BUILD-06 partial); `crates/settings-ui/src/crash_report_dialog.rs` still hard-codes the issues URL; `oneterm-state` cannot log the docks.json recovery itself (no `log` dep) — callers log; literal panel names remain in `Panel::panel_name()` impls (values identical to the constants); the first `windows-quality` CI run is a cold cache.
