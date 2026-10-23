# Work: The horizontal strip's tab reorders by its title, like the rail

ID: US-0202
Intake: IN-0055
Created: 2026-10-23

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

- [x] On the strip, a drag that starts on the title (the label), on the status icon and on the row's edge starts no drag of its own (terminal-view test `the_strip_row_starts_no_tab_drag_of_its_own` on the real tab row), so the kit tab's `DragPanel` is what the press starts; the real dock with real terminal tabs starts it from the title and from the padding (`verify_the_empty_space_still_wins_over_the_terminal_root_catch_all`).
- [x] A strip tab dragged by its title and dropped on another tab reorders (workspace test `a_strip_tab_dragged_by_its_title_reorders`, the real dock skin, strip layout; the same test drops it on content that takes the drop and the dock is unchanged).
- [x] A strip tab dragged by its title onto an empty Space moves its active terminal in and removes the emptied tab (`verify_the_empty_space_still_wins_over_the_terminal_root_catch_all`, now grabbing the tab by its title, with real terminal panels; the move itself is `a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal`), and dropped on a Space that holds a terminal it changes nothing (the same test, first half, and `a_foreign_dock_panel_dropped_on_terminal_content_never_splits_the_centre`).
- [x] `DragTerminalTab`, `start_tab_drag`, the Space's `DragTerminalTab` drag-over / drop handler and `TerminalPanel::handle_tab_drop` are deleted (no source remained after `US-0198` rework 8 and this packet); `cargo clippy` is clean (no dead code).
- [x] The docs say the same.
- [x] Full `pwsh scripts/ci-local.ps1` passes.
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

Changed: `docs/terminal-split/03-drag-drop.md` (sections 1 to 3, 4's head and 6 rewritten around the dock's `DragPanel`; the payload's history is kept in a paragraph), `05-rendering-theme.md`, `07-roadmap-risks.md` (the tree listing, step 6, R1, R2), `docs/gui-layout.md` (Reorder bullet), the HLD (diagram, the two drag rows), `IN-0055.md` (this packet's entry, the follow-up line). Unchanged and still valid: `docs/agents/structure.md` (names no payload), `docs/agents/crate-dependency-rules.md` (no edge moved).

## Context

- `TerminalPanel::dragged_by` resolves a `DragPanel` to its terminal panel through the workspace dock area; `handle_panel_drop` runs the same `move_tab_into` the old `DragTerminalTab` drop ran, so nothing is lost by the deletion.
- The kit's strip tab starts `DragPanel` on the whole `Tab` (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs`, `render_tabs`: `this.on_drag(drag, ..)` on the tab, and `.drag_over::<DragPanel>` / `.on_drop` for the drop line and the reorder). The row OneTerm draws inside it only has to stop competing.

## Plan

- [x] Records commit: this packet and the intake entry.
- [x] `render_tab_strip` starts no drag; delete `DragTerminalTab`, `start_tab_drag`, the Space's handler and `handle_tab_drop` if no source remains (checked: `grep DragTerminalTab crates` found the strip's row and the collapsed icon as the only sources, the latter removed by `US-0198` rework 8; the payload's only other uses were the Space's handler, `handle_tab_drop` and tests).
- [x] Tests: the strip's title reorders (workspace), the terminal-view strip-row test asserts no tab drag from any part of the row; the tests that named `DragTerminalTab` are re-pointed or deleted with it (`tab_drop_onto_occupied_space_keeps_source_terminal` is deleted: the occupied-Space guard is now asserted by `a_dragged_dock_panel_dropped_on_an_empty_space_moves_its_terminal`, with the session-not-closed check moved there).
- [x] Docs, gate.

Choices left open by the request, decided here:

- *Whole row or title only.* The whole row stops competing (the label, chips, status icon, dot); the `x` close button keeps its press-stop, so a press on it still closes and never drags.
- *Delete the payload.* If no source remains after this packet and `US-0198` rework 8, `DragTerminalTab` is dead code; it is deleted rather than kept for a future use (the smallest code).

## Decisions

No decision record: it applies the rule `US-0196` rework 9 already set for the rail.

## Verification Plan

`cargo test -p oneterm-terminal-view`, `-p oneterm-workspace`; fmt; clippy; the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); a walk of the debug build with `SendInput` on a window the coder spawns (a drag by the title, many grab points); the debug app for `ui-verifier`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- **Tests.** `oneterm-terminal-view --lib` 443 passed, 3 ignored (was 444: the occupied-Space test went into the panel-drop test); `oneterm-workspace --lib` 125 passed, 3 ignored. New or re-pointed: `the_strip_row_starts_no_tab_drag_of_its_own` (label, row edge and status icon of the real strip row start no drag; replaces `the_strip_row_still_drags_the_tab_into_a_space`), `a_strip_tab_dragged_by_its_title_reorders` (workspace, real skin), and the real-dock test `verify_the_empty_space_still_wins_over_the_terminal_root_catch_all` grabs the tab by its title for the drop that fills the Space. The probe panel in `tab_rail_tests.rs` lost its own drag, so it mirrors the real row in every layout.
- **Real input.** Not walked by the coder: the machine's console session was at the Windows lock screen (`LogonUI` running, idle more than a day) for the whole run, so `SendInput` could not reach a window; see the report. The gpui tests above drive the real dispatch with simulated mouse events.
- **Gate.** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, the worktree's own `target`), run once on the final tree of the three packets (after `US-0203`): all checks passed. `cargo test --workspace` 2567 passed, 0 failed, 19 ignored; the three `--ignored --test-threads=1` lines (`oneterm-workspace` 3, `oneterm-session-ui` 1, `oneterm-settings-ui` 1) passed; `oneterm-vt` `vt-paranoid` 869, `regex` 882, `no-default-features` 829 passed; `check-ignored-tests.py`, `verify-dependency-graph.py`, `check-doc-paths.py` (219 paths), `check-english.py` (1157 files), `check-theme-contrast.py` (2067 pairings, the five accepted `KNOWN_BELOW` rows unchanged) and the notices check passed. A comment-only edit followed (`tab_rail.rs`, one test doc line); `cargo fmt --check` and `cargo build -p oneterm-app` were re-run after it. Debug binary: `target\debug\oneterm.exe` in the worktree.

Gaps: on-screen check by `ui-verifier` (strip drag by the title in two themes; a drop onto an empty Space; the drop line between strip tabs).

## Handoff

Implemented by the coder; `ui-verifier` next.
