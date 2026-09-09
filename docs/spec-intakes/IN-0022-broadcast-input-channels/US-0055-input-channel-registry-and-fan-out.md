# Work: Input channel registry and input fan-out

ID: US-0055
Intake: IN-0022
Created: 2026-09-09

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

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0022

## Outcome

A `TerminalView` can join one of the channels A..E through an in-memory registry, and every
keystroke, typed text, paste, and Ctrl+C it sends to its own session is repeated on the
sessions of the other members of that channel, in join order, never on itself and never on a
non-member. No UI yet: joining is only reachable from code and tests.

## Scope

- [ ] In scope: `InputChannel` in `oneterm-core`; `InputChannelRegistry`, `Member`,
  `BroadcastInput` in `oneterm-state` with `init` wired where `AgentRegistry::init` runs;
  `TerminalDeps.input_channels`; the four hooks in `crates/terminal-view`;
  `TerminalView::{join_channel, leave_channel, channel}`; `shutdown` leaves; unit tests.
- [ ] Out of scope: actions, menu, chips, frame, key bindings (US-0056); persistence.

## Acceptance

- [ ] `join` moves a member between channels and orders members by join sequence.
- [ ] `leave` and `close` remove members; `close` returns the removed count.
- [ ] `fan_out` writes the same payload to every peer of the origin's channel, skips the
  origin, writes nothing for a non-member origin, and continues after one peer's write error.
- [ ] A member view's `send_key`, IME commit, paste, and Ctrl+C each reach two peer mock
  sessions and not a third non-member view; the origin's own write path is unchanged.
- [ ] A view without a registry behaves exactly as today.
- [ ] `TerminalView::shutdown` removes the member.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/high-level-design.md` — registry shape,
  fan-out points, crate placement.
- `docs/spec-intakes/IN-0022-broadcast-input-channels/low-level-design/registry-and-fan-out.md`
  — interfaces and edge cases this packet implements.
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` — membership unit.
- `docs/terminal-backend.md` §5 — `TerminalSession` write methods; unchanged.
- `docs/agents/crate-dependency-rules.md` — `oneterm-core` ← `oneterm-actions`,
  `oneterm-state` ← `oneterm-terminal` edges already exist; no new edge.
- `docs/agents/structure.md` — crate map gains the two new modules.

### Documentation Action

- Update required: `docs/agents/structure.md` (new files under `crates/core/src` and
  `crates/state/src`); `docs/architecture.md` if it lists `oneterm-state` globals.

Reason: the registry is a new global other crates will look for.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `AgentRegistry` (`crates/state/src/agent_registry.rs`) is the template: `Global` marker,
  idempotent `init`, `remove_terminal` called from `TerminalView::shutdown`.
- `send_key` / `interrupt` (`crates/terminal-view/src/input/keys.rs`), `paste_text`
  (`input/edit.rs`), and `replace_text_in_range` (`terminal_view/ime.rs`) are the only places
  that turn user input into session writes; IME text deliberately bypasses `send_key`.
- `crates/terminal` test-support has a recording mock session usable for peer assertions.

## Plan

- [ ] `InputChannel` + tests in `oneterm-core`.
- [ ] Registry + tests in `oneterm-state`; `init` beside `AgentRegistry::init`.
- [ ] `TerminalDeps.input_channels`, `fan_out` helper, four hooks, `shutdown` leave.
- [ ] View-level tests with three views and mock sessions.
- [ ] Update `docs/agents/structure.md`; run the gate.

## Decisions

- DEC-0009.

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-state -p oneterm-terminal-view`
- `pwsh scripts/ci-local.ps1`

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

Waiting for owner review of IN-0022 before implementation starts.
