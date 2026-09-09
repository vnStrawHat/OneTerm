# Work: Input channel registry and input fan-out

ID: US-0055
Intake: IN-0022
Created: 2026-09-09

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] In scope: `InputChannel` in `oneterm-core`; `InputChannelRegistry`, `Member`,
  `BroadcastInput` in `oneterm-state` with `init` wired where `AgentRegistry::init` runs;
  `TerminalDeps.input_channels`; the four hooks in `crates/terminal-view`;
  `TerminalView::{join_channel, leave_channel, channel}`; `shutdown` leaves; unit tests.
- [x] Out of scope: actions, menu, chips, frame, key bindings (US-0056); persistence.

## Acceptance

- [x] `join` moves a member between channels and orders members by join sequence.
- [x] `leave` and `close` remove members; `close` returns the removed count.
- [x] `fan_out` writes the same payload to every peer of the origin's channel, skips the
  origin, writes nothing for a non-member origin, and continues after one peer's write error.
- [x] A member view's `send_key`, IME commit, paste, and Ctrl+C each reach two peer mock
  sessions and not a third non-member view; the origin's own write path is unchanged.
- [x] A view without a registry behaves exactly as today.
- [x] `TerminalView::shutdown` removes the member.
- [x] `pwsh scripts/ci-local.ps1` green.

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

- `docs/agents/structure.md` — added `crates/core/src/input_channel.rs` and
  `crates/state/src/input_channel_registry.rs` to the crate map.
- `docs/architecture.md` — `oneterm-state` row now lists broadcast input channel membership
  and the registry file; `InputChannelRegistry::init` added to the idempotent init contract.
- `docs/terminal-backend.md` §5 — reviewed, unchanged: no `TerminalSession` method changed.
- `docs/agents/crate-dependency-rules.md` — reviewed, unchanged: no new crate edge
  (`oneterm-state` already depends on `oneterm-terminal`; the new dev-dependency only enables
  that crate's existing `test-support` feature).

## Context

- `AgentRegistry` (`crates/state/src/agent_registry.rs`) is the template: `Global` marker,
  idempotent `init`, `remove_terminal` called from `TerminalView::shutdown`.
- `send_key` / `interrupt` (`crates/terminal-view/src/input/keys.rs`), `paste_text`
  (`input/edit.rs`), and `replace_text_in_range` (`terminal_view/ime.rs`) are the only places
  that turn user input into session writes; IME text deliberately bypasses `send_key`.
- `crates/terminal` test-support has a recording mock session usable for peer assertions.

## Plan

- [x] `InputChannel` + tests in `oneterm-core`.
- [x] Registry + tests in `oneterm-state`; `init` in the composition root.
- [x] `TerminalDeps.input_channels`, `fan_out` helper, four hooks, `shutdown` leave.
- [x] View-level tests with three views and mock sessions.
- [x] Update `docs/agents/structure.md` + `docs/architecture.md`; run the gate.

## Decisions

- DEC-0009.

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-state -p oneterm-terminal-view`
- `pwsh scripts/ci-local.ps1`

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`):

- `cargo test -p oneterm-core -p oneterm-state -p oneterm-terminal-view` — pass:
  `oneterm-core` 45, `oneterm-state` 37, `oneterm-terminal-view` 273 (2 ignored). Nine tests
  are new: one for `InputChannel`, five for the registry (join order + move, leave/close
  counts, `channels_in` order, fan-out reaches peers only, non-member and lone-member write
  nothing, a failing peer does not stop the loop), three at view level (the four input paths
  reach both peers and not the fourth Space, a view without a registry, `shutdown` leaves).
- `pwsh scripts/ci-local.ps1` — `ci-local: all checks passed` (fmt, clippy `-D warnings`,
  workspace tests, dependency graph, doc paths, English check, completion catalogs,
  third-party notices).

Deviations from the LLD, all forced by the existing code:

- `send_key` returns `Option<Vec<u8>>` (the bytes it wrote) instead of taking the origin id
  and `&TerminalDeps`. The fan-out call then sits in `TerminalView::on_key_down`, where the
  origin id already is, and `input/keys.rs` stays free of registry knowledge. Same for
  `interrupt` and the IME commit, which the view calls directly.
- Paste is reached through the shared `EditCommand` fn pointer (menu, panel action, key
  chord), so it cannot read the view. `EditCommand` gained one parameter, a
  `BroadcastOrigin { id, channels }` copied out of the view — the LLD's own suggestion. The
  three non-input edit commands ignore it.
- `TerminalView::channel` takes `&Context<Self>` rather than `&App`: the channel is keyed by
  the view's `EntityId`, which only a `Context` exposes.
- `join_channel` and `channel` are `#[cfg(test)]` until US-0056 dispatches the actions;
  without a caller the lib build fails `clippy -D warnings` on `dead_code`, and the
  repository uses no `allow(dead_code)`. US-0056 removes the gate.
- `Member` is private to `input_channel_registry`: nothing outside constructs or reads it,
  and `members()` hands out `(EntityId, session)` pairs as the LLD interface list specifies.
- `InputChannelRegistry::init` is wired in `crates/app/src/init.rs` beside the other shared
  globals rather than literally next to the `AgentRegistry::init` call, which lives inside
  the Agent feature's own `init` — channels are not an agent concern.

Gaps: none for this packet. Peer write failures are logged (`report_generated_input` /
`report_best_effort`) and never abort the loop; no UI, actions, chips, frame, key bindings or
persistence — those are US-0056 and out of scope here.

## Handoff

US-0056 can start.
