# Work: Input Channel in the tab's context menu

ID: US-0199
Intake: IN-0022
Created: 2026-10-21

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

- Change type: existing-contract change (where a menu lives)
- Risk lane: normal (UI placement; membership, fan-out and the registry are untouched)
- Spec Intake, when required: IN-0022 (owner request 2026-10-21, translated from Vietnamese: "Move Input Channel from the Terminal's context menu to the tab's context menu.")

## Outcome

The "Input Channel" submenu is no longer in the terminal area's right-click menu. It is in the right-click menu of the **tab**, in all three tab presentations (the horizontal strip, the vertical rail's rows and the collapsed rail's cells, which share one menu), with the same items, labels and behaviour. The unit of membership is still the Space (`DEC-0009`); the tab menu acts on the right-clicked tab's **active Space** for the single-Space items, and the two tab-wide items keep acting on every Space of that tab. The seven actions and their (unbound) Key Bindings entries are unchanged.

## Scope

- [x] In scope: the submenu's builder moves from `input/menu.rs` to the panel module (`panel/channel_menu.rs`, with its tests); `MenuContext` loses `channel`, `tab_spaces` and `tab_has_member`; `tab_context_menu` gains the submenu; the owning docs.
- [x] Out of scope: the registry, fan-out, chips, Space badge and frame; the actions and Key Bindings entries; choosing a non-active Space of a split tab from the tab menu (see Plan); a status-bar segment.

## Acceptance

- [x] The terminal's right-click menu has no "Input Channel" item and keeps its other rows and separators in order.
- [x] The tab's right-click menu has "Input Channel" between Duplicate and Close, after a separator, with the items `channel_menu_items` lists (Channel A..E with the current one marked, Leave / Close for a member, the two tab-wide rows for a tab of two or more Spaces).
- [x] Each item acts on the right-clicked tab, not on whichever tab is active (test: an item applied to an inactive tab moves that tab's active Space and no other).
- [x] An empty tab (no terminal Space) shows no submenu (by construction: `channel_entries` is `None` without an active terminal; not driven by a test, because an empty tab cannot be built without spawning a shell).
- [ ] On screen, by `ui-verifier`: the submenu from a horizontal tab, a vertical row and a collapsed cell; a split tab; the terminal menu without it.
- [x] Full `pwsh scripts/ci-local.ps1` passes.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0022-broadcast-input-channels/IN-0022.md`, `high-level-design.md` (wireframe, data flow 1, module list), `low-level-design/menu-chip-frame.md` (section Context menu), `US-0056-input-channel-menu-chips-and-frame.md` (the submenu's origin and its "tab-strip context menu" out-of-scope line), `docs/decisions/DEC-0009-input-channel-membership-is-per-space.md` (membership per Space; tab-wide items are loops).
- `docs/gui-layout.md` section Broadcast input channels and the terminal-tab menu sentence; `docs/terminal-split.md` section Broadcast input channels; `docs/terminal-split/04-context-menu.md` (no channel mention).
- `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/low-level-design/input.md` section Context menu (already lists the terminal menu without the submenu).

### Documentation Action

- Update required: `IN-0022.md` (surface line, packet link), the HLD (wireframe, data flow, module list), `menu-chip-frame.md` (the menu section), `US-0056` (amended note), `docs/gui-layout.md` (both places), `docs/terminal-split.md`. `DEC-0009` is unchanged: it decides the unit of membership, not where the menu is.

Reason: the menu's location is stated as "the terminal context menu / a Space's context menu" in those docs.

### Reconciliation

Changed: `IN-0022.md` (surface line, packet link), `high-level-design.md` (wireframe, bullet, data flow 1, module list), `low-level-design/menu-chip-frame.md` (amendment note), `US-0056` (amendment note), `docs/gui-layout.md` (tab menu sentence, Broadcast input channels), `docs/terminal-split.md`. Unchanged and still valid: `DEC-0009`, `docs/terminal-split/04-context-menu.md`, the IN-0018 `input.md` (it never listed the submenu).

## Context

- `tab_context_menu` (`panel/tab_title.rs`) is the one menu of a tab; the strip, the vertical row and the collapsed cell all attach it in `render_tab_strip`.
- The old items acted on the right-clicked Space through the registry with the Space's session; the tab menu reaches the same registry through the panel: `view.join_channel` / `leave_channel` and `registry.close` already exist and the tab-wide rows are the panel's `join_tab_to_channel` / `leave_tab_channels`.
- A tab has no "this Space" of its own; everything else a tab shows about a terminal (the directory, the status icon, the recording dot) is read from the **active** Space, so the submenu follows that.

## Plan

- [x] Records commit: this packet and the intake link.
- [x] `panel/channel_menu.rs`: `ChannelMenuItem`, `channel_menu_items`, `channel_item_label` (moved, tests moved with them), `TerminalPanel::active_channel`, `TerminalPanel::apply_channel_item`, and the submenu builder used by `tab_context_menu`.
- [x] `input/menu.rs` and its caller in `terminal_view/render.rs`: drop the submenu and the three `MenuContext` fields.
- [x] Tests, docs, gate, debug binary.

Choices left open by the request (decided here, for review):

- *Which Space, in a split tab.* The right-clicked tab's **active Space** (`TerminalPanel::active_view`), the same one the tab's directory, status icon and recording dot report and the one the keyboard actions (`JoinInputChannel` ...) already act on. Membership stays per Space (`DEC-0009`): "Channel B" puts that one Space in B. A user who wants another Space of a split tab in a channel focuses it first, or uses "Join All Spaces In Tab To X" (needs the active Space to be a member, as before). The old terminal menu could address any Space by right-clicking it; that reach is the cost of the move and is recorded as a known loss. A per-Space picker in the tab menu was not built: the request moves one submenu, and the tab chips already list every channel present in a split tab.
- *Submenu for an empty tab.* Not shown: an empty Space cannot join a channel (`IN-0022`).
- *Shortcut hints.* The Join / Leave / Close rows keep their action hints (the bindings are unchanged); the click runs the item against the right-clicked tab directly, as the old rows did against the right-clicked Space.
- *Focus after a click.* The tab menu's other rows do not move focus, so these do not either (the old rows re-focused the terminal because the menu was the terminal's).

## Decisions

No new decision record; `DEC-0009` (membership per Space) is respected and unchanged.

## Verification Plan

`cargo test -p oneterm-terminal-view` (the moved menu-item tests, the new tab-menu tests), clippy, fmt, `check-doc-paths`, `check-english`, the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`), the debug app for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- **Tests.** `oneterm-terminal-view`: the four item-table tests moved with their code to `panel/channel_menu.rs` (`a_non_member_space_is_offered_the_five_channels_only`, `leave_and_close_need_a_membership`, `the_tab_wide_items_need_siblings`, `the_current_channel_is_the_only_marked_item`); new `panel/tests/tab_menu_and_card.rs` `the_tab_menus_channel_items_act_on_the_right_clicked_tab` (two tabs, the item applied on the second joins only its active Space, the marker and the Close row follow, Leave leaves the other tab alone, Close empties the channel in every tab). `PopupMenu` keeps its items private, so the placement itself (between Duplicate and Close, the terminal menu without it) is for `ui-verifier`.
- **Removed.** `MenuContext.{channel, tab_spaces, tab_has_member}`, `BroadcastOrigin::channel` (no remaining reader), the submenu builder and its three helpers in `input/menu.rs`.
- **Known loss (recorded in Plan).** A split tab's non-active Space can no longer be addressed from a menu directly; focus it first.
- **Gate.** See the report: `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, worktree `target`), `cargo build -p oneterm-app`.

## Handoff

`ui-verifier` next (debug binary `target\debug\oneterm.exe` in this worktree).
