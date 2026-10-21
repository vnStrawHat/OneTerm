# Work: Resizable, persisted vertical tab rail width

ID: US-0197
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
- Risk lane: normal (presentation inside OneTerm's own renderer seam; the one persisted field, `ui_config.json` `vertical_tabs.width`, already exists, is parsed and clamped on read since `US-0196`, and gains its writer here; no auth, secret, migration or external effect)
- Spec Intake, when required: IN-0055 (owner ruling Q3, 2026-10-20: 140 to 400 px, at most 35 % of the rail's own tab group)

## Outcome

The vertical tab rail's width is resizable. A 4 px handle on the rail's right edge, with a resize cursor, drags the width live (memory only); on mouse-up the preference is stored once in `ui_config.json` `vertical_tabs.width` (the elevated window applies it in memory and writes nothing). The applied width is the preference clamped to 140 px at least and to at most `max(140, min(400, 35 % of the rail's own tab group's painted width))`; a window resize, the right dock opening or closing, or a centre split changes that group width, and the rail re-clamps from the preference on the next frame, so undoing the change restores the preference. The directory on line 2 of each row follows the live width.

## Scope

- [x] In scope: the pure `clamp_tab_rail_width` with the HLD's six worked examples and the four window examples as unit tests; `UiConfig::set_vertical_tabs_width` (the one writer, clamps to the range, writes nothing when unchanged); per-group painted width read through `on_prepaint` on the group frame, re-clamp on change; the drag handle (4 px, right edge, `ResizeLeftRight` cursor, also while the pointer is dragging past the handle); one write per drag; `draws_rail` evaluated once per frame per group (advisor F4); `docs/gui-layout.md`, `docs/agents/persistence.md`, the HLD.
- [x] Out of scope: collapse (`US-0198`), a double-click-to-reset on the handle, keyboard resizing, resizing from the stored preference when the width is hand-edited out of range (already clamped on read).

## Acceptance

- [ ] `clamp_tab_rail_width` reproduces every worked example of HLD § Width (six group/requested rows and the four window rows) in a unit test.
- [ ] Dragging the handle inside the bounds sets the rail's width live, and on mouse-up the preference is stored (`UiConfig` entity) exactly once: no `UiConfig` update during the move frames, one at the release; a press and release without a move stores nothing.
- [ ] A drag past the ceiling stops there (the applied width equals the ceiling) while the stored preference is the dragged width clamped to 140..=400.
- [ ] Narrowing the window narrows the rail to the new ceiling, and widening it again restores the preference; a split centre bounds each rail by its own group's width.
- [ ] The handle has the resize cursor.
- [ ] The row's directory on line 2 changes width with the rail (test: the path element is wider in a wider rail).
- [ ] An elevated window writes nothing: the setter has no write path of its own, it calls `UiConfig::persist`, which asks `write_refusal` (the existing `an_elevated_window_refuses_to_write_a_perfectly_readable_config`).
- [ ] The width is read, not recomputed, per group: `draws_rail` is evaluated once per frame per group.
- [ ] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] On screen, by `ui-verifier`: drag in a light and a dark theme, the cursor, the handle against the scrollbar with 40 tabs, restart persistence, narrowing and widening the window, a split centre, the elevated window.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md` § Width, § Data Flow 5, § Rail anatomy (right edge row) — the clamp, the worked examples, one write per drag.
- `docs/spec-intakes/IN-0055-vertical-tabs/IN-0055.md` Q3 and the `US-0197` packet entry — the ruling and the acceptance.
- `docs/spec-intakes/IN-0055-vertical-tabs/US-0196-vertical-tab-rail.md` — the carry-forward (deviation 2: `width` stored but inert; F4: `draws_rail` three times per frame; the 64 px two-line row from rework 2).
- `docs/agents/persistence.md` — the `ui_config.json` row and the elevation rule (`DEC-0019` M4).
- `docs/gui-layout.md` § Vertical tabs, § Dock composition (the right dock's own 35 % rule).

### Documentation Action

- Update required: `docs/gui-layout.md` (Vertical tabs: rendering seam says the width is the clamped preference, a "Resizing" paragraph, the closing "later packets" sentence), `docs/agents/persistence.md` (`width` has its writer), the HLD (Rail anatomy right-edge row, data ownership, wireframe note), this packet and the intake's tick.

Reason: the width becomes user-visible behaviour with a writer and a bound.

### Reconciliation

(filled at completion)

## Context

- Base composes a group as a column and calls the skin's `frame`, `render_tab_bar` and `content_frame` in that order within one `render` (`gpui-base` `tab_group.rs` `Render for TabGroup`). The skin therefore computes the group's frame state (does it draw a rail, at what width) once in `frame` and the two later hooks read it.
- The group's painted bounds come from `ElementExt::on_prepaint` on the frame (a zero-size absolute canvas child). Window-level mouse listeners can only be registered in an element's `paint` (the dock's own resize does this in `DockResizeTracker`); the rail does the same with a `canvas` child that exists only while a drag is on.
- The rail's width never feeds its own bound (the group's width includes the rail), so the extra frame requested when the group width changes cannot loop.

## Plan

- [x] Records commit: this packet.
- [ ] `UiConfig::set_vertical_tabs_width`.
- [ ] `tab_rail`: `clamp_tab_rail_width` and its tests; the group's bounds, the live drag width, the handle, the drag tracker; width used by the rail and the content margin; `draws_rail` once per frame.
- [ ] Workspace tests: drag, one write, ceiling, narrow and widen, split centre, cursor, line-2 path follows the width.
- [ ] Docs, gate, debug binary.

Choices left open by the HLD (decided here):

- *The stored value of a drag past the ceiling* is the dragged width clamped to 140..=400 (HLD § Width: `preference = clamp(dragged width, MIN, MAX)`), not the ceiling: the user asked for "as wide as possible", and a wider window then honours it. The rail shows the ceiling meanwhile.
- *Drag arithmetic.* The requested width is the pointer's x minus the group's left edge (the rail is flush left in its group), rounded to a whole pixel when stored.
- *Handle placement.* 4 px, inside the rail's right edge over its 1 px border (the rail clips its children), so it never reaches the content frame's drop handling. It sits over the right 4 px of the list's overlay scrollbar; the scrollbar's thumb is wider than that, so it stays reachable (for `ui-verifier` to confirm with 40 tabs).
- *Cursor during the drag* is held for the whole window by the tracker (`set_window_cursor_style`) so it does not flicker back to an arrow once the pointer leaves the 4 px band.

## Decisions

No decision record: the bound and the preference/applied split are the HLD's (owner ruling Q3).

## Verification Plan

`cargo test -p oneterm-workspace`, `-p oneterm-settings`; clippy; fmt; `check-doc-paths`, `check-english`; full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); debug app for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

(filled at completion)

## Handoff

`ui-verifier` next (debug binary `target\debug\oneterm.exe` in this worktree). After acceptance tick `US-0197` in the intake. `US-0198` (collapse) builds on `tab_rail.rs` and the stored `vertical_tabs.collapsed`.
