# Work: Input Channel menu, tab chips, Space frame, and key bindings

ID: US-0056
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

From a Space's context menu the user joins channel A..E, leaves, closes the channel, or
applies a join or leave to every Space in the tab. Every member Space shows a frame in its
channel colour, every tab shows one chip per channel present in it, and the seven actions
are bindable in Settings › Key Bindings. Depends on US-0055.

## Scope

- [ ] In scope: actions in `crates/actions`; `on_action` handlers and
  `join_tab_to_channel` / `leave_tab_channels` / `tab_channels` on `TerminalPanel`;
  `MenuContext` fields and the "Input Channel" submenu; `channel_color` in
  `crates/terminal-view/src/theme`; chips in `tab_title.rs`; `space_border_color` with the
  channel branch and the single-Space frame; registry observer on the panel; `BindableAction`
  entries; owning-doc updates.
- [ ] Out of scope: status-bar segment; persistence; a tab-strip context menu.

## Acceptance

- [ ] Submenu shows `Channel A..E` with the current one marked; `Leave Channel` and
  `Close Channel <X>` only for a member; the two tab-wide items only when the tab has more
  than one Space (join-all only when this Space is a member, leave-all only when any Space
  in the tab is a member).
- [ ] Member Spaces draw a 1 px frame in the channel colour (full for the active Space,
  55 % for inactive ones); a lone member Space in a single-Space tab draws it too;
  non-members are pixel-identical to today.
- [ ] Tab chips list the distinct channels of the tab in A..E order and disappear when the
  tab has no member.
- [ ] `Close Channel` from one tab repaints every other tab's chips and frames.
- [ ] Seven actions appear in Settings › Key Bindings, group "Input Channel", unbound.
- [ ] E2E: a tab split into three Spaces (two in A, one non-member) plus a second tab in A;
  `echo hi` typed once appears in the three members only. Screenshots in `evidence/`.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/low-level-design/menu-chip-frame.md`
  — rules this packet implements.
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md`.
- `docs/terminal-split.md` — Space tree, Close Space, drag-into-Space.
- `docs/gui-layout.md` — context menu and tab strip composition.
- `README.md` § Features — user-facing feature list.

### Documentation Action

- Update required: `docs/terminal-split.md` (membership follows the view entity; new
  Spaces are non-members; the alternate-screen note), `docs/gui-layout.md` (submenu, chips,
  frame), `README.md` (feature bullet under Terminal split).

Reason: three owning docs describe surfaces this packet changes.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `DuplicateSession` is the end-to-end pattern: `actions!` entry, `BindableAction`, menu
  item with `.action(...)` for the shortcut hint, `on_action` on the panel root.
- `render_tab_strip` already hosts a conditional per-tab badge (the recording dot).
- `space_border_color` is a pure function with a truth-table test in `space/tests.rs`.
- The popup menu has no radio mark; the `* ` prefix marks the current channel.

## Plan

- [ ] Actions + `BindableAction` entries.
- [ ] Panel handlers and tab-wide helpers; registry observer.
- [ ] `MenuContext` fields and submenu; menu tests.
- [ ] `channel_color`, chips, frame; tests.
- [ ] Docs, GUI evidence, gate.

## Decisions

- DEC-0009.

## Verification Plan

- `cargo test -p oneterm-terminal-view -p oneterm-settings-ui -p oneterm-actions`
- GUI walk with PrintWindow screenshots (light and dark theme).
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

Blocked on US-0055 and on owner review of IN-0022.
