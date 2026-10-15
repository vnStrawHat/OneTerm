# Work: Right dock follows the tab kind

ID: US-0194
Intake: IN-0054
Created: 2026-10-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] Switching local -> SSH tab opens the dock, SSH -> local closes it (defaults).
- [ ] Toggling the dock on a tab changes only that kind's remembered choice, and it survives a restart (`ui_config.json`).
- [ ] Switching between two tabs of the same kind never moves the dock.
- [ ] `Ctrl+Shift+B` toggles the dock; it is listed in Settings > Key Bindings and denied in an elevated window.
- [ ] An old `ui_config.json` with `right_dock_mode` loads without quarantine and drops the field on write.
- [ ] Full `scripts/ci-local.sh` passes.

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

- [ ] Settings field + removal of `RightDockMode` / `SetRightDockMode` / mode logic / title-bar control.
- [ ] Workspace observers (kind change -> apply; dock change -> record) + start-up apply + `ToggleRightDock`.
- [ ] Key binding row + elevated policy; tests; docs; gate.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

Unit tests for the pure decision (kind change / same kind / record) and the settings round trip; a workspace gpui test driving `AppState` kind changes and dock toggles; the elevated-policy classification test; full `scripts/ci-local.sh`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
