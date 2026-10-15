# Work: Remove the Agent Panel and OSC 20308

ID: US-0191
Intake: IN-0049
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
- Change type: removal (feature withdrawal)
- Risk lane: high-risk — changes how saved `ui_config.json` / `docks.json` are read; withdraws a published protocol.
- Spec Intake, when required: IN-0049

## Outcome

The Agent Panel, `AgentRegistry`, the agent focuser, OSC 20308 with its `9;7` alias, the right dock's Agent mode, the stale-threshold setting and the demo emitters are gone. Old `ui_config.json` and `docks.json` load with no quarantine, prompt or placeholder (`right_dock_mode: "agent"` reads as `none`).

## Scope

- [x] In scope: everything in `low-level-design/agent-panel-removal.md` § Deleted outright and § Kept, with a changed meaning; the doc moves and updates listed there.
- [x] Out of scope: OSC 7501 (`US-0192`), the tab icon (`US-0193`).

## Acceptance

- [x] `crates/agent-ui` is gone and no crate references 20308, `AgentRegistry`, `agent_panel` or `RightDockMode::Agent`.
- [x] `right_dock_mode: "agent"` loads as `None` and saves as `"none"` (test).
- [x] `agent_stale_threshold_ms` in `ui_config.json` loads without quarantine (test).
- [x] A `docks.json` with an `agent_panel` right dock loads with no `agent_panel` left in the dock area (test).
- [x] `OSC 20308;0` gets no reply (test: the route is the engine's `Drop`, `handle.rs`).
- [x] Full `scripts/ci-local.sh` passes.

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

- [x] Delete code bottom-up (terminal, state, terminal-view, workspace, settings, app, crate), compile after each layer.
- [x] Compat tests for the two documents.
- [x] Docs, scripts, dependency policy; gate (full `ci-local` deferred to the end of the intake, see Evidence).

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

- Removed: `crates/agent-ui`; `oneterm-state` `agent_registry` / `agent_model` / `agent_focus` and the focuser in `AppServices`; `oneterm-terminal` `osc_agent/`, the 20308 / `9;7` routes, `SessionEvent::AgentStatus`, the agent counters and the now-unused `serde` / `serde_json` dependencies; `oneterm-terminal-view` `agent.rs`, `agent_status.rs`, the registry dependency and the dead tab-ordering helpers; the Agent segment of the title bar; `UiConfig::agent_stale_threshold_ms`; the demo scripts and `screenshots/agents-panel.png`. `RightDockMode::None` carries `#[serde(alias = "agent")]`.
- Compat tests: `oneterm-settings` `a_config_from_the_agent_panel_era_loads_and_heals` (`"agent"` -> `None` -> written `"none"`, stale field dropped, no quarantine); `oneterm-workspace` `a_saved_agent_panel_right_dock_is_replaced_by_the_ssh_client_panel`. Both pass.
- `cargo clippy --workspace --all-targets -D warnings`: clean. `cargo test --workspace`: 75 suites ok, 0 failed. `check-ignored-tests.py`, `verify-dependency-graph.py`, `third-party-notices.py --check`, `check-doc-paths.py`, `check-english.py`, `check-theme-contrast.py`: pass.
- Docs: the two specs moved to `docs/archive/` with a withdrawn banner; README, AGENTS.md, PROJECT.md, docs/README.md, structure.md, persistence.md (alias + dropped field + `agent_panel` replacement), architecture.md, gui-layout.md, terminal-backend.md, osc-sequences-checklist.md updated.
- Removed test: the CORR-02 regression (`exited_behind_output_batch_marks_agent_ended`) asserted through the Agent registry, the only thing exit changed. `US-0193` re-adds it against the tab status (exit drops `working`).
- `CARGO_BUILD_JOBS=6 bash scripts/ci-local.sh --full` on Windows, 2026-10-15, after all three packets: all 32 checks passed, 141 test suites / 5045 passed / 0 failed; `cargo deny`: advisories, bans, licenses ok.
- Walk (debug build, scratch working directory): the title bar shows `SSH Client | None` only (`us0193-title-bar-and-tab.png`).
- Closed gap: `crates/vt/docs/guide/05-osc.md` was rewritten around 7501 in `US-0192`. Former gap: `crates/vt/docs/guide/05-osc.md` still uses 20308 as its `Forward` example and calls it OneTerm's agent channel; `US-0192` rewrites it around 7501. The full `scripts/ci-local.sh` runs once at the end of the intake. The engine's own verification tests (`crates/vt/tests/us0098_*.rs`) keep 20308 as an arbitrary unknown number; they test the route table, not OneTerm.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
