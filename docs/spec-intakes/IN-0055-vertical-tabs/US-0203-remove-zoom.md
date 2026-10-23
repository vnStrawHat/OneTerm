# Work: Remove the zoom feature

ID: US-0203
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

- Change type: existing-contract change (removal; owner request, 2026-10-23)
- Risk lane: normal. It touches one persisted document (`docks.json`), but only by ceasing to write one field and ignoring it on read; an existing file loads as before. No auth, secret, migration or external effect.
- Spec Intake, when required: IN-0055 (the zoom trap of `US-0196` rework 9 is what the owner stopped tolerating)

## Outcome

Owner request, 2026-10-23 (translated from Vietnamese): "remove the zoom feature entirely". "Zoom" here is **panel zoom**, the dock feature that makes one tab group fill the dock area (the kit's `ToggleZoom` action, the Maximize / Minimize control in a tab group's toolbar and its `...` menu, `DockArea::set_zoomed_in`). It is not the terminal's font zoom (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`, `TerminalSettings::zoom_in`), which is a different feature and stays.

After this packet nothing in OneTerm can zoom a panel:

- No panel offers a zoom control: the strip's Maximize / Minimize button and the `...` menu's "Zoom In" row are gone (every OneTerm panel answers `zoomable() == false` and offers no `zoom_control`, so the kit draws neither and refuses a zoom however it is asked for).
- The `ToggleZoom` key binding (`shift-escape`, Settings -> Key Bindings, row "Zoom Active Panel") is gone, with its elevated-window policy row.
- `OneTermWorkspace::new` no longer restores `zoomed_panel`, and the workspace no longer tracks or writes it.
- The right dock's SSH Client panel no longer zooms its node when the SFTP Browser expands (see the choice below).
- The rail's zoom-trap release (`OneTermTabGroupSkin::frame`, merged @42bed51a) is deleted: with no way to zoom there is nothing to release.

Persistence: a `docks.json` that holds `zoomed_panel` (every file written by an earlier build while a panel was zoomed, and the 0.5.2 fixture) still loads; the field is ignored and the next save writes none.

## Scope

- [x] In scope: the `zoomable` / `zoom_control` answers of `TerminalPanel`, `SessionPanel`, `SftpPanel`, `SshClientPanel`; the `toggle_zoom` bindable action and its `elevated_policy` rows and tests; `OneTermWorkspace` (`zoomed_panel`, `restore_zoom`, `restore_zoom_in_dock`, the dock observer's zoom mirror, the save paths' `zoomed_panel` argument); `DockDocument::zoomed_panel` and the read path that tolerates the field; the zoom-trap release in `dock_skin.rs` and the tests of it; `SshClientPanel`'s dock zoom and its `set_zoomed` hook; the docs (`docs/gui-layout.md`, `docs/agents/persistence.md`, `docs/agents/structure.md`, `docs/sftp-browser-design.md`, `README.md`, the HLD zoom row, the DEC-0018 shortcut table as history).
- [x] Out of scope: the terminal's font zoom; the kit (never patched: its `ToggleZoom`, `set_zoomed_in` and zoom control stay in the dependency, unreachable from OneTerm); the SFTP Browser's dual-pane **expand** toggle itself (see the choice below); old intake and review documents, which record history.

## Acceptance

- [ ] No OneTerm panel can be zoomed: each answers `zoomable() == false` and offers no zoom control; with the real dock skin and a strip, no zoom control is drawn, and `ToggleZoom` and `DockArea::set_zoomed_in` leave `is_zoomed()` false (workspace test; `set_zoomed_in` is refused by base because the displayed panel is not zoomable).
- [ ] `Settings -> Key Bindings` has no "Zoom Active Panel" row, `toggle_zoom` is not in the elevated-window policy tables, and no keystroke dispatches `ToggleZoom`; a `ui_config.json` that still holds a `toggle_zoom` override loads and the id is ignored (test).
- [ ] A `docks.json` with `zoomed_panel` (the 0.5.2 fixture, and a document written with the field) loads: the layout is restored, nothing is zoomed, the field is not an error and not kept; a save writes no `zoomed_panel` (`oneterm-state` and `oneterm-workspace` tests; the fixture test is re-pointed).
- [ ] `OneTermWorkspace` has no zoom state and writes none: `save_state*` take no zoom argument.
- [ ] The SSH Client panel has no dock zoom; the SFTP expand toggle still switches the browser to its Local + Remote view (its tests unchanged).
- [ ] The zoom-trap release, `release_zoom`, and the tests that exist only for it are deleted; the reorder tests of the rail still pass.
- [ ] Docs say the same; no live doc names a zoom control, the `shift-escape` binding or `zoomed_panel` as current behaviour (history documents keep it).
- [ ] Full `pwsh scripts/ci-local.ps1` passes.
- [ ] On screen, by `ui-verifier`: no Maximize / Minimize button in the strip toolbar, no "Zoom" row in the `...` menu, `Shift+Esc` does nothing, the SFTP expand toggle in the right dock, a profile whose `docks.json` holds `zoomed_panel` starts unzoomed.

## Documentation

### Owning Docs Reviewed

- `docs/agents/persistence.md` (`docks.json` row): the shell-owned `zoomed_panel` field becomes a tolerated, ignored legacy field. Read first, as the work instruction asks.
- `docs/gui-layout.md`: `## Zoom`, the persistence list, the rail's "never left zoomed" bullet and header bullet, the right-dock bullet that names the SFTP expand toggle.
- `docs/spec-intakes/IN-0055-vertical-tabs/high-level-design.md` § Zoom row and the `frame` bullet; `US-0196` Rework 9 owner test failure (the trap and its release).
- `docs/sftp-browser-design.md` § expand; `README.md` feature list; `docs/agents/structure.md` workspace tree line; `docs/decisions/DEC-0018-...` (the shortcut table; a decision record, left as written with a pointer in this packet).
- `crates/actions/src/elevated_policy.rs` (rustdoc and tables), which `docs/agents/crate-dependency-rules.md`-bound crates share.

### Documentation Action

- Update required: the files above.

Reason: zoom stops being a feature; each live statement of it is removed or rewritten, and the persistence row records how a legacy `zoomed_panel` is handled.

### Reconciliation

Filled in at the end.

## Context

- The kit asks two questions before zooming (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs` `zoom_control`, `crates/base/src/dock/tab_group.rs` `set_zoomed`): the panel's base `zoomable()` (does zooming happen at all; base refuses `set_zoomed(true)` for a displayed panel that says no, "however the zoom was asked for") and the component's `zoom_control()` (where the affordance appears). `zoomable() == false` alone makes the button, the menu row, the `ToggleZoom` action and `set_zoomed_in` all inert; the `zoom_control` overrides are removed with it (the default would be `Some(Menu)`, which `zoomable` already overrides, so each panel answers `None` explicitly to keep the intent readable).
- `DockDocument` flattens every unknown field into `dock_fields`, so dropping `zoomed_panel` from the struct without care would carry the legacy field through the typed round trip as a "dock field" of the layout. The read path removes it explicitly instead.

## Plan

- [ ] Records commit: this packet and the intake entry.
- [ ] Panels: `zoomable` false, no zoom control, in the four panels; delete the `SshClientPanel` zoom code and `set_zoomed` hook.
- [ ] Binding: delete the `toggle_zoom` row, the elevated policy entries and tests.
- [ ] Workspace: delete `zoomed_panel`, the restore, the observer's zoom mirror, the save argument; delete the zoom-trap release and its tests (or re-point the ones that guard the strip).
- [ ] Persistence: `DockDocument` ignores a stored `zoomed_panel` and writes none; tests for both, the fixture test re-pointed.
- [ ] Tests: nothing can zoom (workspace); legacy file loads unzoomed and saves without it (state and workspace); `toggle_zoom` override ignored (settings-ui).
- [ ] Docs, gate, debug binary.

Choices left open by the request, decided here:

- **`zoomable() == false` rather than deleting the overrides.** The kit's trait defaults are `zoomable() == true` and `zoom_control() == Some(Menu)`, so deleting OneTerm's overrides would *add* a zoom row to the `...` menu. Each panel therefore states `zoomable == false`; this is the one line that keeps the kit's own zoom surfaces dark. The kit is never patched.
- **The SFTP Browser's expand toggle stays; it no longer zooms the dock.** The right dock's SFTP expand (`IN-0025`) switched the browser to its Local + Remote dual-pane view and zoomed the dock so the view filled the workspace. The owner's "entirely" covers the zoom; deleting the dual-pane view with it would remove the only way to transfer between the two machines' folders, which nobody asked for, so the view stays and simply opens inside the right dock at the width the dock has (the user can drag the dock wider, as before). The toggle's tooltip and the README no longer say "across the workspace". **Open for the owner**: if the dual pane should also go, or the dock should widen itself on expand, that is a follow-up packet.
- **The zoom-trap release is deleted, not kept.** It existed only because a zoom could survive a switch to the rail. With `zoomable == false` for every panel a group can never be zoomed, `DockArea::set_zoomed_in` is refused by base, and the startup restore no longer runs, so `draws && group.is_zoomed()` is unreachable. Keeping an unreachable branch and its three tests would be dead code and a promise nothing keeps.
- **A legacy `zoomed_panel` is ignored on read, not migrated.** No schema bump: removing a value needs no version, and a document with the field is simply a document from an older build. The field is dropped on the next save. An older OneTerm that reads a file this build wrote finds no zoom, which is the default.

## Decisions

No decision record: it removes a feature on the owner's instruction and records the persistence handling above.

## Verification Plan

`cargo test -p oneterm-state`, `-p oneterm-workspace`, `-p oneterm-settings-ui`, `-p oneterm-actions`, `-p oneterm-app` (and `-p oneterm-sftp-ui` / `-p oneterm-session-ui` / `-p oneterm-terminal-view`, which only lose an override); the three `--ignored --test-threads=1` lines (the elevation policy tests are in them); fmt; clippy; `python scripts/check-doc-paths.py`, `check-english.py`; the full `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`); the debug app for `ui-verifier`.

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
