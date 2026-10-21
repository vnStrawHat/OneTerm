# Work: Collapse the vertical tab rail to a 44 px strip

ID: US-0198
Intake: IN-0055
Created: 2026-10-21

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

- Change type: new capability (UI) on an accepted contract
- Risk lane: normal (presentation inside OneTerm's own renderer seam; the one persisted field, `ui_config.json` `vertical_tabs.collapsed`, already exists, is parsed leniently since `US-0196`, and gains its writer here; no auth, secret, migration or external effect)
- Spec Intake, when required: IN-0055 (owner ruling Q4, 2026-10-20: collapsed rows show the tab's position number with the status icon as a badge and the label in a tooltip)

## Outcome

The vertical tab rail can be collapsed to a 44 px strip and expanded again. A button in the rail header (collapse while expanded, expand while collapsed) flips `vertical_tabs.collapsed` through `UiConfig::set_vertical_tabs_collapsed`: live on the next frame, stored once in `ui_config.json` (default `false` omitted; the elevated window applies it in memory and writes nothing), remembered across a restart and while the Vertical Tabs switch is off. While collapsed the rail is exactly 44 px wide whatever the stored width preference or the group width is, the resize handle is not drawn, and each row is a 44 px cell showing the tab's position number (1, 2, 3 ...) with the OSC 7501 status icon as a corner badge and the full title in a tooltip. Click selects, middle-click closes, right-click opens the tab menu, dragging the row's content drags the tab into an empty Space and the grip still reorders; only rename-by-double-click is gone (the menu's Rename... remains).

## Scope

- [x] In scope: `UiConfig::set_vertical_tabs_collapsed` (the one writer, writes nothing when unchanged); the header button and tooltips; the collapsed header (button, `+`, zoom stacked in 44 px); `VerticalTabs::COLLAPSED_WIDTH` and the pure width selection (`rail_width`); the handle off while collapsed; `TabRowLayout::Collapsed(position)` and the compact row in `render_tab_strip` (number, status badge, recording dot, tooltip, no title/cwd/chips/rename); the collapsed row cell in the rail (44 px, no scrollbar gutter, the grip as an overlay); `docs/gui-layout.md`, `docs/agents/persistence.md`, the HLD.
- [x] Out of scope: hover-to-expand (intake follow-up), a collapse shortcut or app-menu item, animating the width change, channel chips in the collapsed row (see Plan), keyboard tab navigation.

## Acceptance

- [ ] `UiConfig::set_vertical_tabs_collapsed`: default is `false` and is omitted from the file, `true` round-trips, a wrong-typed value loads as `false` without quarantine (existing lenient parse, pinned again), a setter call with the stored value changes nothing, and the entity changes before the write so an elevated window still collapses in memory.
- [ ] Width selection: collapsed is 44 px for any preference and any group width (unit test); expanded is `clamp_tab_rail_width` as before.
- [ ] Collapse and expand are live: the header button flips the rail between its width and 44 px on the next frame, in both directions, rebuilding no panel and ending no session; the content frame's margin follows.
- [ ] Collapsed, the resize handle is not drawn and a drag at the rail's edge resizes nothing; expanding restores the preference's width.
- [ ] Collapsed rows are 44 px cells stacked in tab order, each showing its position number; a click selects, the number follows the position after a reorder or a close.
- [ ] The status icon is a badge of the cell; the recording dot is drawn; the tooltip carries the tab's full title.
- [ ] Every row action works collapsed except rename-by-double-click: middle-click closes, the right-click menu opens (with "Close Tabs Below"), the row content starts `DragTerminalTab`, the grip starts `DragPanel`.
- [ ] The Vertical Tabs switch off while collapsed keeps `collapsed = true`; turning it on again returns to the collapsed rail.
- [ ] Zoom, the `+` menu and the scrolling of 40 collapsed rows still work (header stacked, rows scroll, active row in view).
- [ ] Full `bash scripts/ci-local.sh` passes.
- [ ] On screen, by `ui-verifier`: light and dark themes, expanded and collapsed, the badge against the overlay scrollbar with 40 tabs, the tooltip, restart persistence, the elevated window.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md` § Rail anatomy, § State Machine, § Data Flow 6, § UI Wireframe (collapsed), § Data Ownership.
- `docs/spec-intakes/IN-0055-vertical-tabs/IN-0055.md` Q4 and the `US-0198` packet entry (owner accepted all questions as recommended).
- `docs/spec-intakes/IN-0055-vertical-tabs/US-0197-resizable-rail-width.md` — the template for the width path, the handle and the persistence rules; its handoff names this packet.
- `docs/gui-layout.md` § Vertical tabs; `docs/agents/persistence.md` `ui_config.json` row (the elevation rule, `DEC-0019` M4).
- `docs/agents/crate-dependency-rules.md` (R2, R4) — the shell still reaches the row only through `PanelHandle`; the layout travels in `oneterm-state`.

### Documentation Action

- Update required: `docs/gui-layout.md` (Vertical tabs: a **Collapsing** paragraph, the closing "later packet" sentence), `docs/agents/persistence.md` (`collapsed` has its writer and effect), the HLD (collapsed wireframe and rows, data ownership), this packet and the intake's tick.

Reason: collapsing becomes user-visible behaviour with a writer.

### Reconciliation

Filled in at completion.

## Context

- The collapsed row is the panel's own `title()` drawn under a third `TabRowLayout` value (HLD: "the row element renders its compact form when it reads `collapsed`"), so the drag, middle-click and context menu are the one implementation both layouts use; the rail wraps its `title()` call in the scope as it does for the expanded row and supplies the 1-based position.
- `US-0197`'s width path is `TabRail::begin_frame`; collapsing short-circuits it before the group clamp.

## Plan

- [x] Records commit: this packet.
- [ ] `VerticalTabs::COLLAPSED_WIDTH` and `UiConfig::set_vertical_tabs_collapsed`, with the settings tests.
- [ ] `TabRowLayout::Collapsed(position)`; the compact row and its tooltip in `render_tab_strip`; "Close Tabs Below" for it.
- [ ] `tab_rail`: `rail_width`, the collapsed header and cell geometry, the header button, no handle, no gutter; workspace tests (live flip, handle, numbers, actions, switch-off memory).
- [ ] Docs, gate, debug binary.

Choices left open by the HLD (decided here, for review):

- *Cell size.* 44 x 44 px (a square cell as wide as the strip). The expanded row is 64 px because it carries two lines; a number and a badge need no more than a square.
- *Badge placement.* The status icon sits in the cell's top-right corner (3 px inset) and the recording dot in the top-left (3 px inset); both are the same elements the expanded row draws. The number is centred in the whole cell, so the 12 px reorder grip (shown on hover, as expanded) is an overlay on the cell's left edge, painted after the content so it still wins the press there.
- *Channel chips.* Not drawn collapsed: 44 px cannot hold a chip beside a two-digit number and a badge, and the HLD's wireframe shows none. Broadcast membership stays visible in the expanded rail and the status bar. Recorded as a known loss for the owner.
- *Scrollbar.* No gutter is reserved while collapsed (it would take 16 of 43 px). The overlay scrollbar's thumb can cover part of the badge while the pointer is over it; `ui-verifier` to confirm with 40 tabs.
- *Tooltip.* The full tab label only (not the directory); the status icon keeps its own tooltip, which wins when the pointer is on the badge.
- *Header.* Collapsed: a column of expand button, `+` and zoom, each centred, 1 px divider below. Expanded: the collapse button sits with the zoom button on the right. Icons `PanelLeftClose` / `PanelLeftOpen` (the kit's, as its sidebar uses), tooltips "Collapse Tabs" / "Expand Tabs".
- *Resizing mid-collapse.* The handle and its tracker are not built while collapsed; a drag cannot be in progress when a click on the header button lands, so no drag state needs resetting.

## Decisions

No decision record: the stored field, the state machine and the owner's Q4 ruling are the HLD's.

## Verification Plan

`cargo test -p oneterm-settings`, `-p oneterm-state`, `-p oneterm-terminal-view`, `-p oneterm-workspace`; clippy; fmt; `check-doc-paths`, `check-english`; the full `bash scripts/ci-local.sh` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); the debug app for `ui-verifier`.

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

Filled in at completion.
