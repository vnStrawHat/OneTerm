# Work: Tab status icon

ID: US-0193
Intake: IN-0049
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
- Change type: feature (UI)
- Risk lane: high-risk (shares the intake's lane); the change itself is display-only.
- Spec Intake, when required: IN-0049

## Outcome

A single-Space Terminal Tab shows a rotating loader for working, a pulsing bell for blocked, a check for done and a cross for error, from its terminal's 7501 summary; done / error clear when acknowledged; a split tab shows none.

## Scope

- [x] In scope: `low-level-design/tab-status-icon.md`; `docs/gui-layout.md` tab-strip description.
- [x] Out of scope: notifications or sounds on a state change; icons on split tabs; any other surface.

## Acceptance

- [ ] Every edge case in the tab-icon LLD has a test or a recorded manual check.
- [ ] Screenshot of each state in a real window.
- [ ] Reduced motion stops both animations.
- [ ] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0049-osc-7501/high-level-design.md` and its `low-level-design/` — the design.
- `docs/decisions/DEC-0022-osc-7501-replaces-osc-20308-and-the.md` — the ruling.
- `docs/gui-layout.md` — the tab strip's contents.

### Documentation Action

Choose one and explain it before implementation:

- Update required: `docs/gui-layout.md` gains the status icon in the tab-strip description.

Reason: a new element in the tab strip.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [ ] View state + acknowledgement + event.
- [ ] Panel `tab_status` + icon element.
- [ ] Tests, screenshots, docs.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-terminal-view`; manual `printf` reports per state with screenshots; Claude Code run; full gate.

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
