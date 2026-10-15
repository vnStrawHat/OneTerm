# Work: Remove the Agent Panel and OSC 20308

ID: US-0191
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
- Change type: removal (feature withdrawal)
- Risk lane: high-risk — changes how saved `config.json` / `docks.json` are read; withdraws a published protocol.
- Spec Intake, when required: IN-0049

## Outcome

The Agent Panel, `AgentRegistry`, the agent focuser, OSC 20308 with its `9;7` alias, the right dock's Agent mode, the stale-threshold setting and the demo emitters are gone. Old `config.json` and `docks.json` load with no quarantine, prompt or placeholder (`right_dock_mode: "agent"` reads as `none`).

## Scope

- [x] In scope: everything in `low-level-design/agent-panel-removal.md` § Deleted outright and § Kept, with a changed meaning; the doc moves and updates listed there.
- [x] Out of scope: OSC 7501 (`US-0192`), the tab icon (`US-0193`).

## Acceptance

- [ ] `crates/agent-ui` is gone and no crate references 20308, `AgentRegistry`, `agent_panel` or `RightDockMode::Agent`.
- [ ] `right_dock_mode: "agent"` loads as `None` and saves as `"none"` (test).
- [ ] `agent_stale_threshold_ms` in `config.json` loads without quarantine (test).
- [ ] A `docks.json` with an `agent_panel` right dock loads with no `agent_panel` left in the dock area (test).
- [ ] `OSC 20308;0` gets no reply (test).
- [ ] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0049-osc-7501/high-level-design.md` and its `low-level-design/` — the design.
- `docs/decisions/DEC-0022-osc-7501-replaces-osc-20308-and-the.md` — the ruling.
- `docs/osc-agent-status.md`, `docs/agent-panel-display.md` — withdrawn, archived.
- `docs/agents/persistence.md`, `docs/agents/structure.md`, `docs/gui-layout.md`, `docs/terminal-backend.md`, `docs/osc-sequences-checklist.md`, `docs/architecture.md`, `docs/PROJECT.md`, `docs/README.md`, `README.md`, `AGENTS.md` — describe the panel or 20308.

### Documentation Action

Choose one and explain it before implementation:

- Update required: archive the two withdrawn specs; update every doc listed under Owning Docs Reviewed.

Reason: the feature they describe is removed.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [ ] Delete code bottom-up (terminal, state, terminal-view, workspace, settings, app, crate), compile after each layer.
- [ ] Compat tests for the two documents.
- [ ] Docs, scripts, dependency policy; gate.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

Compat unit tests in `oneterm-settings` / `oneterm-workspace`; `oneterm-terminal` 20308 no-reply test; `grep` sweep; `verify-dependency-graph.py`; full `scripts/ci-local.sh`.

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
