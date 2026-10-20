# Work: Vertical tab rail behind a Settings switch

ID: US-0196
Intake: IN-0055
Created: 2026-10-20

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

- Change type: new capability (UI)
- Risk lane: normal (presentation inside OneTerm's own renderer seam plus one additive optional `ui_config.json` field; no auth, secret, migration or external effect)
- Spec Intake, when required: IN-0055 (owner ruling 2026-10-20: Q1-Q9 accepted as recommended)

## Outcome

With **Vertical Tabs** on (Settings > General > Interface, default off, applies on the next frame, survives a restart) every all-terminal centre tab group draws its tabs as a fixed 220 px **rail** down its left side instead of the kit strip above its terminal: a header with `+` and zoom, one scrolling row per tab. Every per-tab behaviour of the strip works in the rail, no panel is rebuilt and no session ends when the switch flips. Turning it off brings the strip back. The `vertical_tabs: {enabled, width, collapsed}` shape is stored in `ui_config.json` (omitted while default, lenient parse, no write from an elevated window); only `enabled` has a writer and a control in this packet.

## Scope

- [x] In scope: the Q8 layout spike (absolute rail in the group frame, left **margin** on the content frame, each visible terminal resized not overlapped); `UiConfig.vertical_tabs` (all three fields stored, lenient, one writer `set_vertical_tabs_enabled`); the Settings switch; the rail in `OneTermTabGroupSkin` (rail rule: centre + non-empty + every panel a `terminal` panel); rows reusing `TerminalPanel::title` in a vertical variant; selected row fill plus 2 px accent bar; 12 px grip with `AppIcon::GripVertical` and the open-hand cursor; vertical scrolling with the active row kept in view; reorder drop slots on rows and below the last row; base `TabList` / `Tab` roles with position in set; header `+` (the panel's `title_suffix`) and the rail's own Zoom In / Zoom Out button; "Close Tabs Below" while vertical; the drag-priority gpui test; `tab.active.foreground` on `tab_bar.background` in `scripts/check-theme-contrast.py` (floor and `HIERARCHY`); `docs/gui-layout.md`, `docs/agents/persistence.md`.
- [x] Out of scope: resizable width (`US-0197`), collapse (`US-0198`), a right-side rail, an app-menu item or action for the switch, keyboard tab navigation, label-drag reorder, the `AnyDrag` host-drag handlers (no OneTerm user, HLD § Rail anatomy).

## Acceptance

- [ ] Q8 spike: in a gpui test, with the switch on the group's content region starts at the rail's width and the active terminal panel is laid out at (group width - 220 px) wide; flipping the switch changes its width in both directions (resized, not overlapped); the rail is absolutely placed and takes no space in base's column. If this fails: stop and report before any fallback.
- [ ] The switch flips the layout on the next frame in both directions without rebuilding a panel or ending a session (test: same panel entity ids, panel count unchanged).
- [ ] Rail rule: a centre group of terminal panels draws one rail row per tab; the SSH Client group draws no rail; a group holding any non-terminal panel, and any group outside the centre, keeps what it draws today (tests).
- [ ] Row behaviour in the rail: click selects (and scrolls only when selection came from elsewhere); middle-click, `x`, double-click rename, the right-click menu, status icon, channel chips and recording dot all come from the shared row element (the same `render_tab_strip`).
- [ ] Drag priority pinned by a gpui test: dragging the title row starts `DragTerminalTab`, the grip starts `DragPanel`, the `x` starts neither.
- [ ] Reorder: dropping a `DragPanel` on a row inserts at that slot, below the last row appends (last slot for a panel of this group); drop slot logic unit tested.
- [ ] 40 tabs: the rows scroll under the header; a tab that becomes active from elsewhere scrolls into view (test on the scroll offset).
- [ ] Zoom keeps the rail; its button reads Zoom Out while zoomed, Zoom In otherwise; no `...` menu, no dock toggle.
- [ ] "Close Tabs Below" while vertical, "Close to the Right" in the strip (test of the label rule).
- [ ] `ui_config.json`: absent -> off; `{"vertical_tabs":{"enabled":true}}` -> on; garbage (`"x"`, `5`, wrong-typed fields) loads as default with no quarantine and the rest of the document intact; default omitted from the written file; width clamped to 140..=400 on read; an older document heals; an elevated window writes nothing (existing `write_refusal` test plus a setter-level assertion).
- [ ] `scripts/check-theme-contrast.py` passes with `tab_bar.background` added to `tab.active.foreground`'s surfaces, both the floor and the `HIERARCHY` comparison.
- [ ] `docs/gui-layout.md` documents the vertical layout and qualifies the "dock buttons in the tab bar" sentence as the horizontal layout's; `docs/agents/persistence.md` row amended.
- [ ] Full `pwsh scripts/ci-local.ps1` passes; screenshots of both layouts in a light and a dark theme by `ui-verifier` (not this agent's verdict).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0055-vertical-tabs/IN-0055.md` and `high-level-design.md` — the design and the owner ruling.
- `docs/spec-intakes/IN-0055-vertical-tabs/evidence/IN-0055-spec-review.md` — advisor findings, all folded into the HLD.
- `docs/gui-layout.md` — dock composition, tab strip, zoom, lock screen.
- `docs/agents/persistence.md` — the `ui_config.json` row and the elevation rule.
- `docs/agents/crate-dependency-rules.md` (R2, R4) — the shell reaches the row only through `PanelHandle`; no new crate edge.
- `docs/spec-intakes/IN-0049-osc-7501/US-0193-tab-status-icon.md` — the status icon rides the same row; its contrast carry-forward (lightness-only theme corrections).
- `docs/spec-intakes/IN-0054-right-dock-follows-the-tab-kind/high-level-design.md` — the right dock follows the tab kind through `AppState`, not the strip.

### Documentation Action

- Update required: `docs/gui-layout.md` (new "Vertical tabs" section; qualify the dock-buttons sentence), `docs/agents/persistence.md` (`ui_config.json` row gains `vertical_tabs`), `scripts/check-theme-contrast.py` (SURFACES), this packet and the intake (owner ruling, ticks).

Reason: a new user-visible layout, one additive persisted field, one new measured surface.

### Reconciliation

Filled in at completion.

## Context

- Base composes every group as a column and applies `.flex().flex_col()` after the skin's frame and content frame (`tab_group.rs:691-728`), so the rail is positioned absolutely and the content frame gets a left margin (not padding, which would put the rail inside the drop-handled bounds).
- Panel code runs `title()` with no knowledge of its group, so the shell cannot tell a panel "you are in a rail" through the panel. Choice left open by the HLD (it said the row reads `UiConfig`, which would give a terminal tab dropped into the right dock a rail-shaped row inside the kit strip): a small `oneterm_state::TabRowLayout` global, set by the rail only around its own `title()` calls (RAII guard) and read by `render_tab_strip`. Everything that is not a rail row, including the kit's drag preview, keeps the horizontal row. The menu wording is captured at that time into the context-menu closure.
- The rail needs the dock area to know whether its group is in the centre: `OneTermDockSkin` captures the `DockArea`'s weak entity at construction and hands it to every group skin.
- `DragPanelPreview` has private fields (the kit constructs it itself), so the rail draws a small preview of its own for the grip drag.

## Plan

- [x] Records commit: owner ruling in the intake, this packet.
- [ ] Settings: `VerticalTabs` struct (`enabled`, `width`, `collapsed`), lenient parse, skip-while-default, `set_vertical_tabs_enabled`, `read_vertical_tabs(cx)`, tests.
- [ ] Q8 spike: skeleton skin with absolute rail + margin; gpui test of widths. Stop and report on failure.
- [ ] `oneterm_state::TabRowLayout`; vertical row variant in `render_tab_strip`; `Close Tabs Below`.
- [ ] Rail: header, rows (base `Tabs`/`Tab`), grip, scroll, drop slots, accessibility, zoom button.
- [ ] Settings > General switch; icon `grip-vertical.svg`.
- [ ] Tests (rail rule, live switch keeps entities, drag priority, drop slots, scroll, label rule, settings).
- [ ] Contrast script surface; docs; gate; debug binary for ui-verifier.

## Decisions

No decision record: the choices (left rail, one switch, rows drawn by the panel) live in `docs/gui-layout.md`. Plan-level choices left open by the HLD are listed under Context above.

## Verification Plan

`cargo test -p oneterm-settings`, `-p oneterm-state`, `-p oneterm-workspace`, `-p oneterm-terminal-view`, `-p oneterm-settings-ui`; `python scripts/check-theme-contrast.py`; full `pwsh scripts/ci-local.ps1`; a smoke launch of the debug build with a private profile for `ui-verifier`.

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
