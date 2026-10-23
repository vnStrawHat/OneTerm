# Work: Remove the zoom feature

ID: US-0203
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

- [x] No OneTerm panel can be zoomed: `TerminalPanel`, `SessionPanel` and `SftpPanel` answer `zoomable() == false` with no zoom control (tests `the_terminal_panel_offers_no_zoom`, `the_session_panel_offers_no_zoom`, `the_sftp_panel_offers_no_zoom`; `SshClientPanel` answers the same in code, **not pinned by a test**: its constructor needs the vault and the saved-session store, which `oneterm-app` has no test support for); with the real dock skin and a strip, a probe that answers no has no zoom control, `DockArea::set_zoomed_in` and `ToggleZoom` leave `is_zoomed()` false, and a control probe that answers yes shows the button and zooms (`a_panel_that_will_not_zoom_has_no_zoom_control_and_cannot_be_zoomed`).
- [x] `Settings -> Key Bindings` has no "Zoom Active Panel" row, `toggle_zoom` is not in the elevated-window policy tables, and nothing is bound to `shift-escape` (`no_action_zooms_a_panel`); a `ui_config.json` that still holds a `toggle_zoom` override loads, the id is ignored and the next save leaves it out (`a_saved_toggle_zoom_override_is_ignored`).
- [x] A `docks.json` with `zoomed_panel` (the 0.5.2, v0 and v1 fixtures, and a hand-written v2 document) loads: the layout is restored, nothing is zoomed, the field is not an error and not kept as a layout field; a save writes no `zoomed_panel` (`oneterm-state` `a_stored_zoomed_panel_loads_and_is_not_written_back` and the fixture reads; `oneterm-workspace` `pre_migration_fixture_loads_and_saves_without_semantic_drift`, re-pointed: it loads the 0.5.2 fixture unzoomed and the saved file has no `zoomed_panel`).
- [x] `OneTermWorkspace` has no zoom state and writes none: the `zoomed_panel` field, `restore_zoom`, `restore_zoom_in_dock` and the dock observer's mirror are deleted, and `save_state`, `save_state_to` and `save_state_logged` take no zoom argument.
- [x] The SSH Client panel has no dock zoom (`on_sftp_expanded`, `set_zoomed` and the `zoomable` override that said yes are gone); the SFTP expand toggle still switches the browser to its Local + Remote view (the sftp-ui tests are unchanged and pass; the expand tooltip no longer says "across the workspace").
- [x] The zoom-trap release, `release_zoom`, and the tests that exist only for it (`a_zoom_cannot_trap_the_rail`, `the_strip_keeps_its_zoom_and_turning_the_rail_on_releases_it`) are deleted; the reorder tests of the rail still pass.
- [x] Docs say the same; no live doc names a zoom control, the `shift-escape` binding or `zoomed_panel` as current behaviour (history documents keep it).
- [x] Full `pwsh scripts/ci-local.ps1` passes.
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

Changed: `docs/gui-layout.md` (`## Zoom` rewritten as the removal, the panel-trait bullets, the header bullet, the rail's "never left zoomed" bullet deleted, the persistence list, the source map), `docs/agents/persistence.md` (`docks.json` row), `docs/agents/structure.md` (a `zoom` module that does not exist), `docs/sftp-browser-design.md` (4.15 and the panel snippet), `README.md` (the SFTP and status-bar lines), the HLD (Zoom row, `frame` bullet, header row), `docs/terminal-split/01`, `02`, `06`, `07` (the zoom mentions), `DEC-0018` (an amendment line under its table), `US-0196` (rework 9's trap marked superseded), `IN-0055.md` (this packet's entry). Unchanged and still valid: `docs/agents/crate-dependency-rules.md` (no edge moved).

## Context

- The kit asks two questions before zooming (`reference/gpui-kit/crates/component/src/dock/tab_panel.rs` `zoom_control`, `crates/base/src/dock/tab_group.rs` `set_zoomed`): the panel's base `zoomable()` (does zooming happen at all; base refuses `set_zoomed(true)` for a displayed panel that says no, "however the zoom was asked for") and the component's `zoom_control()` (where the affordance appears). `zoomable() == false` alone makes the button, the menu row, the `ToggleZoom` action and `set_zoomed_in` all inert; the `zoom_control` overrides are removed with it (the default would be `Some(Menu)`, which `zoomable` already overrides, so each panel answers `None` explicitly to keep the intent readable).
- `DockDocument` flattens every unknown field into `dock_fields`, so dropping `zoomed_panel` from the struct without care would carry the legacy field through the typed round trip as a "dock field" of the layout. The read path removes it explicitly instead.

## Plan

- [x] Records commit: this packet and the intake entry.
- [x] Panels: `zoomable` false, no zoom control, in the four panels; delete the `SshClientPanel` zoom code and `set_zoomed` hook.
- [x] Binding: delete the `toggle_zoom` row, the elevated policy entries and tests.
- [x] Workspace: delete `zoomed_panel`, the restore, the observer's zoom mirror, the save argument; delete the zoom-trap release and its tests (or re-point the ones that guard the strip).
- [x] Persistence: `DockDocument` ignores a stored `zoomed_panel` and writes none; tests for both, the fixture test re-pointed.
- [x] Tests: nothing can zoom (workspace); legacy file loads unzoomed and saves without it (state and workspace); `toggle_zoom` override ignored (settings-ui).
- [x] Docs, gate, debug binary.

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
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- **Removed.** `zoomable() == false` and `zoom_control() == None` in `TerminalPanel`, `SessionPanel`, `SftpPanel` and `SshClientPanel`; the `toggle_zoom` bindable row and its two `elevated_policy` entries; `OneTermWorkspace::{zoomed_panel, restore_zoom}` and `restore_zoom_in_dock`; the `zoomed_panel` argument of `save_state`, `save_state_to` and `save_state_logged`; `OneTermTabGroupSkin::release_zoom` and its `frame` hook; `SshClientPanel::on_sftp_expanded`'s dock zoom, its `set_zoomed` hook and its `dock_area` field; `dock_util::{find_tab_node_by_panel_name, panel_name_for_tab_node}` (their only callers were the zoom restore and the zoom mirror).
- **Persistence.** `DockDocument::zoomed_panel` is replaced by a private, never-serialized `_legacy_zoomed_panel` (`#[serde(rename = "zoomed_panel", skip_serializing)]`), so a stored zoom is taken out of the flattened dock fields at every read (the typed read, the shared update transaction and the straight `from_str` alike) and no write carries it. No schema bump. An elevated window never read it anyway (`startup_dock_document`).
- **Tests.** `oneterm-state` 79 passed; `oneterm-actions` 4; `oneterm-settings-ui` 60 passed, 1 ignored; `oneterm-workspace` 124 passed, 3 ignored; `oneterm-terminal-view` 444 passed, 3 ignored; `oneterm-sftp-ui` 78; `oneterm-session-ui` 102 passed, 1 ignored; `oneterm-app` 26. The existing `dock_persistence` tests that used `zoomed_panel` as a marker now use a dock field (`mark` / `marker`).
- **The zoom-trap release is dead, not kept.** `draws && group.is_zoomed()` needed a group that could be zoomed; none can: base refuses `set_zoomed(true)` for a displayed panel whose `zoomable()` is false (`reference/gpui-kit/crates/base/src/dock/tab_group.rs` `set_zoomed`), the component's `ToggleZoom` handler refuses when the control is `None`, and the startup restore is gone. The control probe in `a_panel_that_will_not_zoom_...` shows the kit does zoom a panel that says yes, so the refusal asserted for the real answer is the load-bearing one.
- **Gate.** `pwsh scripts/ci-local.ps1` (`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, the worktree's own `target`), run once on the final tree of the three packets (after `US-0203`): all checks passed. `cargo test --workspace` 2567 passed, 0 failed, 19 ignored; the three `--ignored --test-threads=1` lines (`oneterm-workspace` 3, `oneterm-session-ui` 1, `oneterm-settings-ui` 1) passed; `oneterm-vt` `vt-paranoid` 869, `regex` 882, `no-default-features` 829 passed; `check-ignored-tests.py`, `verify-dependency-graph.py`, `check-doc-paths.py` (219 paths), `check-english.py` (1157 files), `check-theme-contrast.py` (2067 pairings, the five accepted `KNOWN_BELOW` rows unchanged) and the notices check passed. A comment-only edit followed (`tab_rail.rs`, one test doc line); `cargo fmt --check` and `cargo build -p oneterm-app` were re-run after it. Debug binary: `target\debug\oneterm.exe` in the worktree.
- **Real binary, no input.** A scratch profile (cwd and `USERPROFILE` in the scratchpad) holding the 0.5.2 fixture as `target/docks.json` (it stores `"zoomed_panel": "terminal"`) and a `ui_config.json` with a stale `toggle_zoom` override: the debug build started, ran 12 s, and its next `docks.json` had no `zoomed_panel`; the `ui_config.json` was left as it was (it is rewritten, without the stale id, the next time the bindings are saved). The pid was stopped.

Gaps: `SshClientPanel`'s `zoomable() == false` is not pinned by a test (see Acceptance). The kit's own `ToggleZoom` action and `set_zoomed_in` still exist in the dependency; they are unreachable from a OneTerm panel but a future panel that forgets to answer `zoomable() == false` would inherit the kit default (zoomable, a menu row). On-screen check by `ui-verifier`.

## Handoff

Implemented by the coder; `ui-verifier` next.
