# Work: Right dock follows the tab kind

ID: US-0194
Intake: IN-0054
Created: 2026-10-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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

- Change type: new capability | existing-contract change | bug | maintenance | acceptance rework
- Change type: feature (UI behaviour) + removal
- Risk lane: normal — one additive `ui_config.json` field; the removed `right_dock_mode` is ignored by owner ruling, no migration code.
- Spec Intake, when required: IN-0054

## Outcome

The title bar has no right-dock mode control. The right dock opens or closes when the active tab's kind (local / SSH) changes, to the choice remembered for that kind (defaults: local closed, SSH open); a dock button or `Ctrl+Shift+B` changes and persists the choice for the current kind.

## Scope

- [x] In scope: everything in the HLD's Data Flow, Persistence and Removed sections; the bindable action and its elevated-policy classification; docs.
- [x] Out of scope: auto-collapse by window width; per-host or per-session memory; changing the SSH Client panel itself.

## Acceptance

- [ ] Switching local -> SSH tab opens the dock, SSH -> local closes it (defaults). Decision unit-tested; not yet seen in a running window (see Gaps).
- [x] Toggling the dock on a tab changes only that kind's remembered choice, and it survives a restart (`ui_config.json`).
- [x] Switching between two tabs of the same kind never moves the dock.
- [x] `Ctrl+Shift+B` toggles the dock; it is listed in Settings > Key Bindings and denied in an elevated window.
- [x] An old `ui_config.json` with `right_dock_mode` loads without quarantine and drops the field on write.
- [x] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0054-right-dock-follows-the-tab-kind/high-level-design.md` — the design and rulings.
- `docs/gui-layout.md` — title bar contents, right dock behaviour.
- `docs/agents/persistence.md` — the `ui_config.json` row.
- `docs/decisions/DEC-0018-app-shortcuts-leave-single-ctrl-keys-to-the-terminal.md` — `ctrl-shift-b` is not a bare Ctrl key, so no amendment.
- `docs/decisions/DEC-0019-elevated-shells-open-in-an-elevated-window.md` — an elevated window has no right dock; the new action is classified as denied.

### Documentation Action

Choose one and explain it before implementation:

- Update required: `docs/gui-layout.md` (title bar, right dock behaviour), `docs/agents/persistence.md` (`right_dock`).

Reason: the title bar loses a control and the dock gains automatic behaviour and a persisted choice.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] Settings field + removal of `RightDockMode` / `SetRightDockMode` / mode logic / title-bar control.
- [x] Workspace observers (kind change -> apply; dock change -> record) + start-up apply + `ToggleRightDock`.
- [x] Key binding row + elevated policy; tests; docs; gate.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

Unit tests for the pure decision (kind change / same kind / record) and the settings round trip; a workspace gpui test driving `AppState` kind changes and dock toggles; the elevated-policy classification test; full `scripts/ci-local.sh`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- Removed: the title bar's segmented control and its `UiConfig` observer, `SetRightDockMode`, `oneterm_core::RightDockMode`, `panel_names::right_dock_panel_name`, and the workspace's `switch_right_dock_mode` / `apply_right_dock_mode` / `reopen_or_rebuild` / `right_dock_mode_for` / `sync_right_dock_mode` / `right_dock_panel_mode`.
- Added: `UiConfig.right_dock: RightDockPrefs { local, ssh }` (default false / true, omitted while default); `ToggleRightDock` (bindable "Toggle Right Dock", `ctrl-shift-b`, classified denied for an elevated window); the workspace observes `AppState` and opens / closes the dock only when the active tab's kind changes (`right_dock_open_on_kind_change`), and records a dock-area open state that differs from the remembered choice for the kind the dock was last set for (`right_dock_prefs_after_toggle`); start-up applies the active kind.
- Tests: `the_right_dock_moves_only_when_the_tab_kind_changes`, `a_dock_toggle_is_remembered_for_the_kind_it_was_made_on`, `opening_and_closing_the_right_dock_keeps_its_panel_and_width` (replaces the mode-switch and reopen tests); `oneterm-settings` round trip of `right_dock`, default not written, old `right_dock_mode` / `agent_stale_threshold_ms` ignored and dropped; the elevated-policy exhaustiveness test covers the new action.
- Docs: `gui-layout.md` (right dock behaviour, title bar, elevated window, caching note; also the `agent_panel` / `AgentListView` sentence US-0191 missed), `persistence.md`, `architecture.md`, README.
- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh --full` (2026-10-15): all checks passed, 141 suites / 5040 passed / 0 failed (five fewer than before: the mode-toggle tests went, three tests came).
- Gaps: not walked in a running window — the workstation is locked. The observer wiring (AppState -> dock, dock -> `ui_config.json`) is proven by the decision tests and the compile, not by a GUI run; no test constructs a whole `OneTermWorkspace` (it reads and writes the real `docks.json`).

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
