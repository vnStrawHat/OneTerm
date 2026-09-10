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
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: normal
- Spec Intake, when required: IN-0022

## Outcome

From a Space's context menu the user joins channel A..E, leaves, closes the channel, or
applies a join or leave to every Space in the tab. Every member Space shows a badge with its
channel letter, every tab shows one chip per channel present in it, and the seven actions
are bindable in Settings › Key Bindings. Depends on US-0055.

## Scope

- [x] In scope: actions in `crates/actions`; `on_action` handlers and
  `join_tab_to_channel` / `leave_tab_channels` / `tab_channels` on `TerminalPanel`;
  `MenuContext` fields and the "Input Channel" submenu; `channel_color` in
  `crates/terminal-view/src/theme`; chips in `tab_title.rs`; the member badge in
  `space/render.rs`; registry observer on the panel; `BindableAction`
  entries; owning-doc updates.
- [x] Out of scope: status-bar segment; persistence; a tab-strip context menu.

## Acceptance

- [x] Submenu shows `Channel A..E` with the current one marked; `Leave Channel` and
  `Close Channel <X>` only for a member; the two tab-wide items only when the tab has more
  than one Space (join-all only when this Space is a member, leave-all only when any Space
  in the tab is a member).
- [x] Member Spaces draw the channel badge in their top-right corner, the lone Space of an
  unsplit tab included; Space borders keep the theme's active/inactive rule for members and
  non-members alike, and the single-Space fast path stays borderless.
- [x] Tab chips list the distinct channels of the tab in A..E order and disappear when the
  tab has no member.
- [x] `Close Channel` from one tab repaints every other tab's chips and badges.
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
- [x] `channel_color`, chips, badge; tests.
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

## Rework 2026-09-09

Owner feedback on `evidence/US-0056-split-broadcast-dark.png` (translated): "I cannot tell
apart the case of one tab with three Spaces where only one Space joined channel A."

Cause: the frame is the only per-Space marker, and in the dark theme the channel-A colour
(`chart_1`) is nearly the same blue as `table_active_border`, the active-Space border, while
the inactive member's 55 % frame reads like the plain theme border. A frame colour alone
cannot say which Space is a member.

Change: every member Space now draws the tab chip again as a badge in its top-right corner
(bold channel letter, `channel_color`, text `cx.theme().background`), inset 3 px from the top
and 15 px from the right so it clears the terminal's 12 px scrollbar track. It renders in both
paths of `space/render.rs` (split and the single-Space fast path, which becomes `.relative()`),
takes no keyboard focus and carries no click handler. The chip builder moved from
`panel/tab_title.rs` into `theme/input_channel.rs` (`channel_chip` plus the pure
`channel_chip_style`), so the tab chip and the Space badge are one function. The frame rule is
unchanged and is now the secondary cue.

New test: `every_channel_chip_shows_its_letter_on_its_channel_color`
(`theme/input_channel.rs`) asserts the label/background/foreground triple for A..E.

Docs updated with the change: this packet, `low-level-design/menu-chip-frame.md` (Space badge
section, interfaces, verification), `high-level-design.md` (wireframe and bullets),
`docs/gui-layout.md`, `docs/terminal-split.md`.

Evidence (rework GUI walk, described in `evidence/US-0056-gui-walk.md`):
`evidence/US-0056-rework-one-member-dark.png` (three Spaces, only one in channel A, the active
Space being a non-member), `evidence/US-0056-rework-two-channels-dark.png` (two Spaces in A,
one in B), `evidence/US-0056-rework-one-member-light.png` (the first case in the light theme).

Verification: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1`.

## Rework 2 2026-09-09

Owner feedback: "Remove the coloured frame."

Change: a member Space no longer draws a frame in its channel colour. The top-right badge
added by the first rework is the only per-Space marker, and Space borders return to the
pre-IN-0022 rule for every Space: `table_active_border` for the active Space, `border` for
the inactive ones, and no border at all on the single-Space fast path. That path keeps
`.relative()` and the badge, so a lone member is still marked without changing the pixel
layout of an unsplit tab. `space_border_color(is_active, active, inactive)` is back to its
original signature and its original truth-table test; the channel-branch test is gone, and
`space/render.rs` no longer imports `channel_color` (the chip builder in
`theme/input_channel.rs` still uses it, so the function itself stays).

Docs updated with the change: this packet, `low-level-design/menu-chip-frame.md` (the Space
frame section dropped, the badge section states the border rule), `high-level-design.md`
(Idea, wireframe, bullets), `IN-0022.md` (surfaces and safety bullets), `docs/gui-layout.md`,
`docs/terminal-split.md`, `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md`,
`README.md`.

Evidence (described in `evidence/US-0056-gui-walk.md` under "Rework 2"):
`evidence/US-0056-rework2-one-member-dark.png` (one tab, three Spaces, only one in channel A,
the active Space a non-member) and `evidence/US-0056-rework2-two-channels-dark.png`
(two Spaces in A, one in B). Pixel sampling of both confirms the member borders equal the
theme rule.

Verification: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1`.

## Rework 3 2026-09-10

Owner feedback: "The channel chip on the tab title is not square", then "put the Space
badge at top 5 px, right 5 px".

Change: `channel_chip` is a fixed 16 px square (`size`, flex-centred letter, rounded
corners kept) instead of a padded box whose width followed the letter, so the tab chip and
the Space badge share one footprint. The badge moved from 3 px / 15 px to 5 px from the top
and right edges of the Space; it now overlaps the scrollbar track, which only shows a thumb
at the top when the view is scrolled up. Docs updated: `low-level-design/menu-chip-frame.md`
and `docs/terminal-split.md`.

Evidence: `cargo test -p oneterm-terminal-view` and `pwsh scripts/ci-local.ps1` (recorded
below in Evidence and Gaps). No new screenshot: the owner placed the badge by hand in the
running build and asked for exactly that offset.

## Handoff

IN-0022 is complete as specified: US-0055 (registry + fan-out) and US-0056 (menu, chips,
frame, key bindings) are implemented. Open follow-ups stay out of scope: a status-bar
channel segment, persistence of membership (see DEC-0009), and a tab-strip context menu.
