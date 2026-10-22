# Work: A terminal leaves its Input Channel when its session ends

ID: US-0200
Intake: IN-0022
Created: 2026-10-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (the lifetime of a membership)
- Risk lane: normal (in-memory membership only; no persisted schema, no secret handling change; it removes a target from a fan-out, never adds one)
- Spec Intake, when required: IN-0022 (owner ruling 2026-10-22, translated: "Yes" to "when a terminal's shell process exits, it should leave its Input Channel automatically; today it stays a member until its tab closes and broadcasts to it fail with a logged warning.")

## Outcome

When a Space's session ends, its `TerminalView` leaves its input channel through the same path the menu's "Leave Channel" uses (`TerminalView::leave_channel`), so the registry notifies and every `TerminalPanel` repaints: the tab chip, the Space badge and the frame go, the menu shows the Space as a non-member, and a later broadcast from another member no longer targets the dead session. A member that is still running is untouched. If the leaver was the last member of its channel, the channel is simply empty (there is no separate open/closed state, HLD data flow 5), exactly as after a manual leave.

## Scope

- [x] In scope: `SessionEvent::Exited` and `SessionEvent::Closed` in `TerminalView::handle_event` call `leave_channel`; tests; the docs that state the membership lifetime.
- [x] Out of scope: blocking a manual join of an exited Space (see Plan, "join of a dead Space"); a restart-on-exit feature (OneTerm has none); any change to the registry, to `fan_out` or to the tab-wide rows; persistence (channels are not persisted).

## Acceptance

- [ ] `Exited` makes a member leave: `channel_of` is `None` afterwards (view test), and a non-member's `Exited` is a harmless no-op.
- [ ] A local shell and an SSH session both leave: SSH reports `Exited` only when the server sends an exit status or signal, and always reports `Closed` last, so a dropped connection (EOF, channel close, disconnect, no exit status) is covered by `Closed` (test: `Closed` alone makes a member leave).
- [ ] On a two-Space tab where one Space exits: the tab chip and the registry's member list lose it, the other member keeps its chip, and input typed in the live member reaches only live members (the dead session's probe records no write).
- [ ] A lone member that exits leaves an empty channel: the tab has no chip (same as a manual leave).
- [ ] A Space that exited and is joined again by hand stays a member (no automatic rejoin and no block) until its tab closes; `shutdown` still leaves.
- [ ] Full `pwsh scripts/ci-local.ps1` passes; `cargo build -p oneterm-app` builds.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/IN-0022.md`, `high-level-design.md` (data flow 6, "Data ownership and lifecycle"), `low-level-design/registry-and-fan-out.md` (hook table, edge cases: a peer that has exited), `US-0055`, `US-0056`, `US-0199` and `evidence/US-0199-verify.md` (the advisor's Info: "a member whose process exits stays a member until its tab is closed ... one `leave_channel` in the `Exited` arm plus a test, as a separate packet"; this is that packet).
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` (unit of membership, no persistence): unchanged, it decides who a member is, not when it stops being one.
- `docs/gui-layout.md` "Broadcast input channels", `docs/terminal-split.md` "Broadcast input channels (IN-0022)".
- `crates/terminal/src/backend/pump.rs` (`publish_exit*`, `publish_closed*`), `crates/local-shell/src/event_loop.rs:674-685`, `crates/ssh/src/task.rs:93-144`: where the two events come from.

### Documentation Action

- Update required: `IN-0022.md` (packet link, lifecycle sentence), `high-level-design.md` (data flow 6, lifecycle), `low-level-design/registry-and-fan-out.md` (hook note and the exited-peer edge case), `docs/gui-layout.md` and `docs/terminal-split.md` (a member leaves when its session ends). `DEC-0009` unchanged.

Reason: the docs say a Space is removed on leave, close and `shutdown` only.

### Reconciliation

Filled in at completion.

## Context

- **Event source.** `TerminalView::handle_event` (`crates/terminal-view/src/terminal_view/view.rs`), arms `SessionEvent::Exited(_)` and `SessionEvent::Closed`. Local shell: the event loop publishes `Exited(code)` then `Closed` for every end (`event_loop.rs:683,685`). SSH: `Exited` only on `ChannelMsg::ExitStatus` / `ExitSignal` (`task.rs:93-100`); EOF, channel close and a dropped connection break the loop and only `Closed` follows (`task.rs:144`). OneTerm has no "press Enter to restart" and never respawns a session inside a view: a new shell is a new tab and a new `TerminalView`, which starts as a non-member.
- **The existing leave path.** `TerminalView::leave_channel` calls `registry.leave(id)`, which returns `false` and does not notify for a non-member, so calling it from both events is idempotent and cheap. `TerminalPanel` observes the registry (`terminal_panel.rs`), so chips and frames repaint without further wiring.

## Plan

- [ ] `Exited` and `Closed` arms call `self.leave_channel(cx)`. Both, not only `Exited`: the owner's rule is the session ending; `Exited` alone would miss an SSH drop, and `Closed` alone would delay the leave on an SSH server that sends an exit status but keeps the channel open.
- [x] Choice, rejoin: **no automatic rejoin.** Nothing restarts a session in place, so there is nothing to rejoin; a new tab is a non-member and the user joins it from the tab menu.
- [x] Choice, persistence: nothing to do. Membership is in memory only (DEC-0009), the registry is empty at start, and sessions are not restored, so an exited member cannot be resurrected.
- [x] Choice, join of a dead Space: **not blocked.** A manual join (or "Join All Spaces In Tab") of an exited Space still works and the Space stays a member until its tab closes, as US-0199 recorded; a broadcast to it fails as a logged warning, as before. Blocking it would turn menu rows into silent no-ops and was not asked. Follow-up for the owner if wanted.
- [ ] Tests (see Verification Plan).

## Decisions

No decision record: it is a lifetime rule inside DEC-0009's model and the Plan records the choices.

## Verification Plan

- `cargo test -p oneterm-terminal-view`: view tests for `Exited` and `Closed` (member leaves, non-member harmless, running member untouched, dead member not written to by a peer's input), a panel test on a two-Space tab (chip and member list), the lone-member case, and the rejoin-by-hand case.
- Regression: `shutdown_leaves_the_channel`, `member_input_reaches_the_channel_peers_only`, `tab_channel_helpers_cover_every_space_of_the_tab`.
- Full gate `pwsh scripts/ci-local.ps1`; `cargo build -p oneterm-app`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Filled in at completion.

## Handoff

None.
