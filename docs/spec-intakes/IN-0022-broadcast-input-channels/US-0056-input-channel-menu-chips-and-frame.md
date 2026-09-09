# Work: Input Channel menu, tab chips, Space frame, and key bindings

ID: US-0056
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

From a Space's context menu the user joins channel A..E, leaves, closes the channel, or
applies a join or leave to every Space in the tab. Every member Space shows a frame in its
channel colour, every tab shows one chip per channel present in it, and the seven actions
are bindable in Settings › Key Bindings. Depends on US-0055.

## Scope

- [x] In scope: actions in `crates/actions`; `on_action` handlers and
  `join_tab_to_channel` / `leave_tab_channels` / `tab_channels` on `TerminalPanel`;
  `MenuContext` fields and the "Input Channel" submenu; `channel_color` in
  `crates/terminal-view/src/theme`; chips in `tab_title.rs`; `space_border_color` with the
  channel branch and the single-Space frame; registry observer on the panel; `BindableAction`
  entries; owning-doc updates.
- [x] Out of scope: status-bar segment; persistence; a tab-strip context menu.

## Acceptance

- [x] Submenu shows `Channel A..E` with the current one marked; `Leave Channel` and
  `Close Channel <X>` only for a member; the two tab-wide items only when the tab has more
  than one Space (join-all only when this Space is a member, leave-all only when any Space
  in the tab is a member).
- [x] Member Spaces draw a 1 px frame in the channel colour (full for the active Space,
  55 % for inactive ones); a lone member Space in a single-Space tab draws it too;
  non-members are pixel-identical to today.
- [x] Tab chips list the distinct channels of the tab in A..E order and disappear when the
  tab has no member.
- [x] `Close Channel` from one tab repaints every other tab's chips and frames.
- [x] Seven actions appear in Settings › Key Bindings, group "Input Channel", unbound.
- [x] E2E: a tab split into three Spaces (two in A, one non-member) plus a second tab in A;
  `echo hi` typed once appears in the three members only. Screenshots in `evidence/`.
- [x] `pwsh scripts/ci-local.ps1` green.

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

- `docs/terminal-split.md` — new "Broadcast input channels (IN-0022)" section: membership
  follows the view entity, new Spaces from Split / Duplicate Session are non-members, the
  frame rule for members, and the alternate-screen note (broadcast is not suppressed there).
- `docs/gui-layout.md` — new "Broadcast input channels" section (submenu order, chips, Space
  frame, the registry observer) plus three source-map entries.
- `README.md` — feature bullet under "Terminal split (Spaces)".
- `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` — Status `accepted`.
- `docs/spec-intakes/IN-0022-broadcast-input-channels/low-level-design/menu-chip-frame.md` —
  reviewed, unchanged: the implementation follows it except for the deviations listed below.

## Context

- `DuplicateSession` is the end-to-end pattern: `actions!` entry, `BindableAction`, menu
  item with `.action(...)` for the shortcut hint, `on_action` on the panel root.
- `render_tab_strip` already hosts a conditional per-tab badge (the recording dot).
- `space_border_color` is a pure function with a truth-table test in `space/tests.rs`.
- The popup menu has no radio mark; the `* ` prefix marks the current channel.

## Plan

- [x] Actions + `BindableAction` entries.
- [x] Panel handlers and tab-wide helpers; registry observer.
- [x] `MenuContext` fields and submenu; menu tests.
- [x] `channel_color`, chips, frame; tests.
- [x] Docs, GUI evidence, gate.

## Decisions

- DEC-0009.

## Verification Plan

- `cargo test -p oneterm-terminal-view -p oneterm-settings-ui -p oneterm-actions`
- GUI walk with PrintWindow screenshots (light and dark theme).
- `pwsh scripts/ci-local.ps1`

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, `CARGO_TARGET_DIR=target/in22-target` because the user's own
OneTerm holds `target/`):

- `cargo test -p oneterm-terminal-view -p oneterm-settings-ui -p oneterm-actions` — pass
  (306 passed, 2 ignored). New tests: `channel_menu_items` conditions and
  `channel_item_label` (`input/menu_tests.rs`), `space_border_color` channel branch
  (`space/render.rs`), `channel_color` mapping (`theme/input_channel.rs`),
  `tab_channel_helpers_cover_every_space_of_the_tab` (`panel/tests.rs`), and
  `the_seven_input_channel_actions_ship_unbound` (`key_bindings_actions.rs`).
- `pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`.
- GUI walk with PrintWindow screenshots in `evidence/`, described in
  `evidence/US-0056-gui-walk.md`: the three-Space split with two members of channel A, the
  second tab in the same channel, `echo hi` typed once reaching the three members only, the
  submenu, the two-chip tab strip, the repaint after `Close Channel`, and the same layout in
  the light theme.

Deviations from the LLD:

- The submenu items call the registry (and, for the two tab-wide items, the panel) directly
  instead of dispatching the actions. The menu belongs to the Space that was right-clicked,
  which is not necessarily the active one, while the actions act on the active Space — the
  same reason the Split and Duplicate items already call the panel. The items still carry
  their action for the shortcut hint, exactly like `Split Right`.
- Non-current channel labels are indented by two spaces (`"  Channel A"`) so the `* ` marker
  of the current one does not shift the column.
- `SpaceTree::render` takes the registry handle as a parameter. Reading it out of the panel
  inside `render_leaf` would re-enter the panel entity that is being rendered.
- `MenuContext.tab_spaces` counts terminal Spaces (`TerminalPanel::terminal_space_count`),
  not leaves: an empty Space cannot join a channel.
- The item list is a pure `channel_menu_items` + `channel_item_label` pair (as the LLD's
  fallback allows) because `PopupMenu` keeps its items private.

Gaps: the GUI walk ran while the workstation was locked, so it used posted window messages;
the seven actions were bound temporarily in the scratch `target/ui_config.json` to reach them
by key (they ship unbound, as the Key Bindings test asserts). No status-bar segment and no
persistence — out of scope here.

## Handoff

IN-0022 is complete as specified: US-0055 (registry + fan-out) and US-0056 (menu, chips,
frame, key bindings) are implemented. Open follow-ups stay out of scope: a status-bar
channel segment, persistence of membership (see DEC-0009), and a tab-strip context menu.
