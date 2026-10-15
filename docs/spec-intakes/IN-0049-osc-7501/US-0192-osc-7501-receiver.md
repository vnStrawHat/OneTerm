# Work: OSC 7501 receiver

ID: US-0192
Intake: IN-0049
Created: 2026-10-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

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

- Change type: new capability | existing-contract change | bug | maintenance | acceptance rework
- Change type: feature
- Risk lane: high-risk — parses untrusted bytes (possibly remote) and sends a new reply to programs.
- Spec Intake, when required: IN-0049

## Outcome

OneTerm answers `OSC 7501 ; ?`, keeps the rev 0.3 record table per session, drops malformed or oversized reports whole, and emits `SessionEvent::ProgramStatus(summary)` at most once per batch when the summary changes.

## Scope

- [x] In scope: `low-level-design/osc-7501-receiver.md` (route, parser, record table, router wiring, event type); checklist rows in `docs/osc-sequences-checklist.md`; `docs/terminal-backend.md` router row.
- [x] Out of scope: any UI (`US-0193`); `RIS` handling (recorded deviation); OSC 9;4 mapping; showing `msg` / `title`.

## Acceptance

- [x] Every edge case in the receiver LLD has a passing test.
- [x] The probe reply precedes DA1 in one batch.
- [x] No `oneterm-vt` public-API diff (`vt-public-api.py --check`).
- [ ] E2E: Claude Code >= 2.1.295 reports in a local tab and in an SSH tab (Q8 settled and recorded).
- [ ] Full `scripts/ci-local.sh` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0049-osc-7501/high-level-design.md` and its `low-level-design/` — the design.
- `docs/decisions/DEC-0022-osc-7501-replaces-osc-20308-and-the.md` — the ruling.
- `docs/osc-sequences-checklist.md` Group G — 7501 rows.
- `docs/terminal-backend.md` — the `OscRouter` responsibilities.

### Documentation Action

Choose one and explain it before implementation:

- Update required: add 7501 to the OSC checklist; add the 7501 duty to the router row in `terminal-backend.md`.

Reason: a new supported OSC and a new router duty.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [x] `program_status.rs` + tests (parser, table).
- [x] Route + router wiring + event; backend tests.
- [ ] Docs; E2E with Claude Code.

## Decisions

Link consequential decisions; do not repeat their rationale here.

## Verification Plan

`cargo test -p oneterm-terminal`; `vt-public-api.py --check`; Claude Code E2E local and SSH; full gate.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `crates/terminal/src/program_status.rs`: parser (rev 0.3 grammar, id rules, base64 + control-character refusal for `title` / `msg`, 4096-byte ceiling), `RecordTable` (replace, subtree / bare `clear`, LRU at 64, prompt start), summary by `ProgramState` order. `program_status_tests.rs`: 12 tests, one per LLD edge case.
- Router: 7501 arm (truncated dropped, probe answered in the same pass, report applied), `PromptStart` drops transient records, summary published once per batch and only for batches that carried a 7501 report or a prompt start (no extra lock otherwise). `adapter_config`: `route(7501, Forward).large(7501, true)`; the route test pins it as the only override.
- Backend tests through the real engine: probe-then-DA1 and DA1-then-probe leave in byte order with the probe's terminator; 1000 reports in one batch give one event, an unchanged summary none, a malformed report none, bare `clear` gives `None`; prompt start drops `working` and keeps `done`; a 2.7 KB report crosses the engine, a 4.1 KB one is dropped.
- `cargo test -p oneterm-terminal -p oneterm-terminal-view`: 183 + 397 passed. Workspace clippy clean. `vt-public-api.py --check --no-doc`: unchanged. `cargo test -p oneterm-vt --doc guide`: 34 passed (the guide's forwarding example now uses 7501; the `9;7` wrap example is reframed as a hypothetical private sub-code).
- Docs: `osc-sequences-checklist.md` (7501 rows, policy paragraph, status summary, engine note), `terminal-backend.md` (router and state rows, tree), `structure.md`.
- Gaps: E2E (Claude Code, and Q8 ConPTY ordering) runs with `US-0193`'s manual walk and is recorded there and here when done. The view ignores `ProgramStatus` until `US-0193`.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
