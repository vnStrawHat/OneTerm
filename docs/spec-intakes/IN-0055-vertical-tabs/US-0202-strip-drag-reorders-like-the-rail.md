# Work: The horizontal strip's tab reorders by its title, like the rail

ID: US-0202
Intake: IN-0055
Created: 2026-10-23

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

- Change type: existing-contract change (owner request, 2026-10-23)
- Risk lane: normal (a drag source in OneTerm's own tab row; no persisted state, no auth, secret or external effect)
- Spec Intake, when required: IN-0055 (the rail's drag rules, `US-0196` rework 9 and `US-0198` rework 8, extended to the strip)

## Outcome

With Vertical Tabs off, the horizontal tab strip behaves like the rail: dragging a tab **by its title** reorders it (the kit's `DragPanel`, as the tab's padding and edge already did); dropping it on an **empty Space** moves its terminal in (the rework 9 path, `TerminalPanel::handle_panel_drop`); dropping it on **non-empty terminal content** does nothing (the existing "terminal content never splits or merges the dock" rule). So the tab's label no longer starts OneTerm's own `DragTerminalTab`.

Owner report (translated from Vietnamese, 2026-10-23): the horizontal tab strip should drag and reorder like the vertical rail.

Today the strip's row (`render_tab_strip`, horizontal layout) starts `DragTerminalTab` from the whole row, label included, so a drag by the title moves the tab into a Space but reorders nothing, and only the kit tab's own padding or edge (outside the row) starts the reorder `DragPanel`. It is the same two-zone split that made the collapsed cell "sometimes reorder, sometimes not" (`US-0198` rework 8).

After this packet and `US-0198` rework 8 nothing starts `DragTerminalTab` any more; the payload type, its preview and the Space's drop handler for it are deleted (see Plan).

## Scope

- [x] In scope: `render_tab_strip` (no drag on the strip's row); the empty-Space placeholder's `DragTerminalTab` drag-over / drop handler, `TerminalPanel::handle_tab_drop`, the `DragTerminalTab` payload, `start_tab_drag` and the tests that name them, once no source remains; the probe panels and tests of the strip; `docs/gui-layout.md`, `docs/terminal-split/03-drag-drop.md` (and the sibling split docs that name the payload), the HLD rows.
- [x] Out of scope: the kit's own strip (drop line, preview chip, reorder rule) and the rail; any change to what a non-empty terminal does with a drop; the right dock.

## Acceptance

- [ ] On the strip, a drag that starts on the title (the label), on the status icon, on the chips and on the tab's padding all start the reorder `DragPanel`; none starts a `DragTerminalTab` (terminal-view test on the real tab row, workspace test on a probe that mirrors it).
- [ ] A strip tab dragged by its title and dropped on another tab reorders (workspace test on the real dock skin, strip layout).
- [ ] A strip tab dragged by its title onto an empty Space moves its active terminal in and removes the emptied tab (the existing `a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal`), and dropped on a Space that holds a terminal it changes nothing (the existing `a_foreign_dock_panel_dropped_on_terminal_content_never_splits_the_centre` rule).
- [ ] `DragTerminalTab` and its Space handler are deleted when no source remains; `cargo clippy` is clean (no dead code).
- [ ] The docs say the same.
- [ ] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] On screen, by `ui-verifier`: drag a strip tab by its title to reorder; drag it onto an empty Space.

## Documentation

### Owning Docs Reviewed

- `docs/gui-layout.md` § Vertical tabs (Reorder bullet, the collapsed cell bullet) and § Tabs: the strip's drag wording.
- `docs/terminal-split/03-drag-drop.md` §§ 1-6: the payload's definition, the placeholder's hooks, "why `DragTerminalTab` stays" (§ 6).
- `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md` rows "Drag a tab into an empty Space" and "Reorder / move between groups"; `IN-0055.md` follow-up "Reordering by dragging the label itself".
- `US-0196` rework 9 and `US-0198` rework 8: the rail's rules this packet extends.

### Documentation Action

- Update required: the files above, so that no doc says the strip's title drags the tab into a Space or that `DragTerminalTab` exists. `docs/terminal-split/03-drag-drop.md` § 6 is rewritten: the rejected-then-accepted history of the payload becomes "the dock's `DragPanel` is the only drag; an empty Space resolves its panel".

Reason: the contract "a Space accepts `DragTerminalTab` from the strip's title" is replaced by "a Space accepts the dock's `DragPanel` of a terminal tab from the strip, the rail and the collapsed cell alike".

### Reconciliation

Filled in at the end.

## Context

- `TerminalPanel::dragged_by` resolves a `DragPanel` to its terminal panel through the workspace dock area; `handle_panel_drop` runs the same `move_tab_into` the old `DragTerminalTab` drop ran, so nothing is lost by the deletion.
- The kit's strip tab starts `DragPanel` on the whole `Tab` (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs`, `render_tabs`: `this.on_drag(drag, ..)` on the tab, and `.drag_over::<DragPanel>` / `.on_drop` for the drop line and the reorder). The row OneTerm draws inside it only has to stop competing.

## Plan

- [ ] Records commit: this packet and the intake entry.
- [ ] `render_tab_strip` starts no drag; delete `DragTerminalTab`, `start_tab_drag`, the Space's handler and `handle_tab_drop` if no source remains (check first).
- [ ] Tests: the strip's title reorders (workspace), the terminal-view strip-row test asserts no tab drag from any part of the row; the tests that named `DragTerminalTab` are re-pointed or deleted with it.
- [ ] Docs, gate.

Choices left open by the request, decided here:

- *Whole row or title only.* The whole row stops competing (the label, chips, status icon, dot); the `x` close button keeps its press-stop, so a press on it still closes and never drags.
- *Delete the payload.* If no source remains after this packet and `US-0198` rework 8, `DragTerminalTab` is dead code; it is deleted rather than kept for a future use (the smallest code).

## Decisions

No decision record: it applies the rule `US-0196` rework 9 already set for the rail.

## Verification Plan

`cargo test -p oneterm-terminal-view`, `-p oneterm-workspace`; fmt; clippy; the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); a walk of the debug build with `SendInput` on a window the coder spawns (a drag by the title, many grab points); the debug app for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Filled in at the end.

## Handoff

Implemented by the coder; `ui-verifier` next.
