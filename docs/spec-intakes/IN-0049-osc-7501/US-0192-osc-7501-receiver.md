# Work: OSC 7501 receiver

ID: US-0192
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
- Change type: feature
- Risk lane: high-risk — parses untrusted bytes (possibly remote) and sends a new reply to programs.
- Spec Intake, when required: IN-0049

## Outcome

OneTerm answers `OSC 7501 ; ?`, keeps the rev 0.3 record table per session, drops malformed or oversized reports whole, and emits `SessionEvent::ProgramStatus(summary)` at most once per batch when the summary changes.

## Scope

- [x] In scope: `low-level-design/osc-7501-receiver.md` (route, parser, record table, router wiring, event type); checklist rows in `docs/osc-sequences-checklist.md`; `docs/terminal-backend.md` router row.
- [x] Out of scope: any UI (`US-0193`); `RIS` handling (recorded deviation); OSC 9;4 mapping; showing `msg` / `title`.

## Acceptance

- [ ] Every edge case in the receiver LLD has a passing test.
- [ ] The probe reply precedes DA1 in one batch.
- [ ] No `oneterm-vt` public-API diff (`vt-public-api.py --check`).
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

- [ ] `program_status.rs` + tests (parser, table).
- [ ] Route + router wiring + event; backend tests.
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

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
