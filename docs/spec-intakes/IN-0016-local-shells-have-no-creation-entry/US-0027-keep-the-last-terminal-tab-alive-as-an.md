# Work: Keep the last terminal tab alive as an empty placeholder

ID: US-0027
Intake: IN-0016
Created: 2026-08-25

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: acceptance rework of the existing close-all-tabs behavior
- Risk lane: normal
- Spec Intake, when required: `IN-0016`

## Outcome

Closing the final terminal tab leaves one empty terminal placeholder tab, preserving the tab bar and `+` menu so local terminals and newly connected SSH sessions can be opened visibly.

## Scope

- [x] In scope: terminal close button, middle-click, `ClosePanel` action, final-tab session shutdown, empty placeholder reset, and focused regression tests.
- [x] Out of scope: generic dock behavior, backend connection logic, persistence schema, and automatic creation of a replacement shell.

## Acceptance

- [x] Closing the only terminal tab shuts down its session and retains the `TabPanel` with one empty `TerminalPanel`.
- [x] The retained tab continues rendering the tab bar and its existing `+` New Terminal menu.
- [x] The empty placeholder can still create a local terminal in place.
- [x] Adding a connected SSH `TerminalPanel` after the reset produces a visible sibling tab.
- [x] Closing one terminal tab when siblings exist still removes only that tab.
- [x] Close button, middle-click, context-menu, and keybinding close routes share the same behavior.

## Documentation

### Owning Docs Reviewed

- `docs/PROJECT.md` — terminal-view and GPUI UI verification boundaries.
- `docs/terminal-split.md` — current shipped Space-tree overview.
- `docs/terminal-split/02-split-and-close.md` — existing last-Space/last-tab behavior.
- `docs/terminal-split/06-integration.md` — current `TerminalPanel` to `TabPanel` close mapping.
- `docs/agents/error-policy.md` — session shutdown must remain observable and non-panicking.
- `reference/gpui-component/crates/ui/src/dock/tab_panel.rs` — pinned generic tab close/action behavior.

### Documentation Action

Update required: `docs/terminal-split/02-split-and-close.md` and `docs/terminal-split/06-integration.md` currently require the final Space to remove its tab, which conflicts with the accepted retained-placeholder behavior.

Reason: final terminal-tab lifecycle is an established user-visible contract and must match the implementation.

### Reconciliation

Updated `docs/terminal-split/02-split-and-close.md` and
`docs/terminal-split/06-integration.md` with the retained final-tab policy. Updated
`docs/agents/ui-fork-maintenance.md`, `docs/agents/structure.md`, and
`vendor/README.md` for the reviewed read-only `TabPanel::panel_count` fork delta.

## Context

The generic `TabPanel::remove_panel` detaches itself from its parent stack when empty. Terminal-specific close entry points currently call it directly, and generic `ClosePanel` actions bypass terminal state. The existing `SpaceTree::new_empty` placeholder already supplies the desired retained content.

## Plan

- [x] Add focused tests for final-tab reset and normal multi-tab removal.
- [x] Route terminal-tab close entry points through one terminal-owned helper.
- [x] Reconcile the split docs and run focused plus full quality gates.

## Decisions

No new decision record: this behavior is scoped to the accepted terminal UI outcome and does not establish a cross-feature architectural policy.

## Verification Plan

- Focused GPUI regression tests in `crates/terminal-view/src/panel/tests.rs`.
- `cargo test -p oneterm-terminal-view`.
- `scripts/ci-local.sh` (or PowerShell equivalent) for the mandatory CI-equivalent gate.
- `git diff --check` and `srcwalk review` for changed-line review.
- Manual Windows GUI close/add-local/add-SSH flow if available.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Focused close regressions passed: 2 tests cover retaining/resetting the only tab,
  closing its session exactly once, adding an SSH panel afterward, and preserving
  normal removal when a sibling exists.
- `cargo test -p oneterm-terminal-view`: 193 tests passed.
- `bash vendor/refresh.sh --check`: all vendored crates match pristine sources plus
  their reviewed patch sets.
- `CARGO_BUILD_JOBS=1 bash scripts/ci-local.sh`: full required local CI gate passed,
  including fmt, clippy with warnings denied, workspace tests, dependency/UI/doc/
  English/catalog/notices checks.
- The first parallel `scripts/ci-local.sh` attempt reached workspace tests but hit
  Windows error 1455 (`The paging file is too small`). The serial rerun passed; this
  was an environment resource limit rather than a source/test failure.
- `git diff --check` and the scoped `srcwalk review` passed.
- Manual Windows GUI E2E was not run, so exact visual rendering and a real SSH connect
  after the reset remain explicit verification gaps.

## Handoff

Current owner: implementation session. No blocker.
