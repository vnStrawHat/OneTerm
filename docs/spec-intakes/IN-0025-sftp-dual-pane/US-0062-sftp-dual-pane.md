# Work: SFTP Browser expand/collapse with a Local pane

ID: US-0062
Intake: IN-0025
Created: 2026-09-11

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework — done 2026-09-11)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high_risk (local delete / overwrite; see the LLD)
- Spec Intake, when required: IN-0025

## Outcome

The SFTP Browser has an expand/collapse toggle. Collapsed is today's remote-only browser.
Expanded shows a Local pane (left) and the Remote pane (right) in a resizable split with the
transfer queue below; the Local pane browses and manages local directories, and files move
between the panes with Upload / Download, double-click, and drag & drop. The expanded flag
and local directory survive a restart.

## Scope

- [x] In scope: `SftpTableState.expanded` + `local_dir`; toolbar toggle; `LocalPane`
  (listing, sort, navigation, path box, New Folder / Rename / Delete / Refresh, context menu);
  Upload from local selection (button, menu, double-click, drag); Download to local cwd while
  expanded (menu, drag) with overwrite confirmation; local refresh after a download; tests;
  docs (README, persistence, structure, design pointer).
- [x] Out of scope: a Windows drive list (type `D:\` in the local path box); synchronized
  browsing; multi-select; local Properties / Edit; key bindings for local actions; widening
  or zooming the right dock on expand.
- [x] Added for verification only: `crates/tools/src/bin/sftp-dev-server.rs`, a loopback
  SSH + SFTP developer diagnostic (no sshd or container exists on the Windows box), so the
  GUI walk could run against a real `SftpBackend` session.

## Acceptance

- [x] Toggle in the SFTP toolbar; collapsed renders the existing layout unchanged (GUI:
  `evidence/US-0062-collapsed-dark.png`, `US-0062-collapsed-again-dark.png`).
- [x] **Rework (owner, 2026-09-11):** expand/collapse behaves like the terminal tab
  zoom — expanded fills the whole workspace (dock node zoomed, Session section hidden,
  Maximize/Minimize icons), collapse (or any other zoom-out) docks it again; the zoom
  persists through `docks.json` `zoomed_panel` like a zoomed terminal tab, and the
  no-connection state keeps the toggle reachable (GUI: the five `US-0062-rework-*` captures
  in `evidence/US-0062-gui-walk.md`).
- [x] **Follow-up (owner, 2026-09-11):** the toggle is the panel's `title_suffix`, drawn at
  the trailing end of the "SFTP Browser" section header by `SshClientPanel`, framed like the
  terminal tab bar's trailing control group (full height, left border, tab-bar padding)
  (GUI: `US-0062-title-toggle-*` captures).
- [x] Expanded: Local (left) / Remote (right) resizable split, transfer queue under both
  (GUI: `US-0062-expanded-dark.png`; superseded by the rework capture).
- [x] Local pane lists the persisted (else home) directory: folders first, sortable Name /
  Date Modified / Size; double-click folder, back, path box + Enter, refresh work; a failed
  listing keeps the rows and shows a banner (unit: `pane_lists_navigates_and_keeps_rows_on_a_failed_listing`,
  `local_sort_is_folders_first_then_by_column`; GUI: listing + refresh after transfers).
- [x] Local New Folder / Rename / Delete work with the confirmations and guards in the LLD
  (GUI: New Folder created, empty name rejected, folder delete confirmed with the recursive
  wording and removed; unit: `entry_names_reject_slashes_and_dot_components` now rejects `\`;
  Rename not walked in the GUI — same dialog and validator as the remote rename).
- [x] Upload: selected local file/folder → remote cwd via button, menu, double-click (file),
  or dragging the row onto the remote list; the remote list refreshes (GUI: button and drag,
  `US-0062-transfers-drag-dark.png`; unit: `local_selection_uploads_into_the_remote_cwd`).
- [x] Download while expanded: selected remote entry → local cwd without a save dialog,
  overwrite confirmed when the target exists; dragging a remote row onto the local list does
  the same; the local list refreshes when the transfer ends. Collapsed keeps the save dialog
  (GUI: menu download, drag download, `US-0062-replace-prompt-dark.png`; unit:
  `expanded_download_targets_the_local_directory_without_a_dialog`,
  `expanded_download_onto_an_existing_file_asks_first`, the pre-existing save-dialog tests).
- [x] `expanded` and `local_dir` persist in `docks.json`; an old document loads collapsed
  (unit: `table_state_defaults_to_collapsed_and_round_trips_the_local_dir`; GUI: a restart
  with `expanded: true` opened in the dual-pane layout, the final collapse wrote `false`).
- [x] `pwsh scripts/ci-local.ps1` green (see Evidence).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0025-sftp-dual-pane/high-level-design.md` — layout, ownership, flows.
- `docs/spec-intakes/IN-0025-sftp-dual-pane/low-level-design/local-mutations.md` — guards.
- `docs/agents/persistence.md` — `docks.json.sftp_table_state` row (gains two fields).
- `docs/agents/structure.md` — `sftp-ui` module listing.
- `docs/gui-layout.md` — right-dock layout; `sftp_table_state` mention is generic, no change.
- `docs/sftp-browser-design.md` — historical record; add a pointer section like §4.14.
- `README.md` § SFTP file browser — feature list.
- `docs/PROJECT.md` — boundaries ("local destinations are contained under the chosen
  directory") still hold: the chosen directory is now the local pane's cwd; the tools crate
  sentence is generic, no change.

### Documentation Action

- Update required: `README.md`, `docs/agents/persistence.md`, `docs/agents/structure.md`,
  `docs/sftp-browser-design.md` (pointer section).

Reason: the persistence row enumerates the field's meaning; the structure doc lists modules;
the README lists SFTP features.

### Reconciliation

- `README.md` — SFTP bullet for the dual-pane Local + Remote view.
- `docs/agents/persistence.md` — `sftp_table_state` row names `expanded` and `local_dir`.
- `docs/agents/structure.md` — `sftp-ui` listing names `local_pane` and `drag`.
- `docs/sftp-browser-design.md` — new §4.15 pointing at this intake.
- `crates/tools/Cargo.toml` description names the new diagnostic binary; `docs/PROJECT.md`
  reviewed, unchanged (generic wording still correct).
- Rework: `high-level-design.md` (idea, diagram, wireframe, data flow 1), `README.md`
  bullet, `docs/gui-layout.md` (`SshClientPanel` is zoomable for the expanded browser),
  `docs/sftp-browser-design.md` §4.15.

## Context

- `SftpPanel::do_upload_paths(Vec<PathBuf>)` uploads into the remote cwd and refreshes —
  the single entry point for uploads (picker, external drop, and now the local pane).
- `SftpPanel::download_to(..)` registers the queue item and drives the transfer; `do_download`
  opens `prompt_for_new_path` only while collapsed.
- Zoom sync (rework): `SftpPanel` emits `SftpExpandedChanged`; `SshClientPanel` (now
  `zoomable`) zooms its dock node through `Window::defer` (zooming reads the panel's
  `zoomable`, which is illegal inside the panel's own update) and implements the base
  `Panel::set_zoomed` hook so any zoom change sets the browser's expanded state. The shell's
  `zoomed_panel` persistence and `restore_zoom` are reused unchanged.
- `SftpTableDelegate::render_tr` returns the row `Stateful<Div>`; `DataTable` chains styling
  onto it, so the `on_drag` attached there survives.
- The remote file list already accepted `ExternalPaths` drops; `LocalRowDrag` is accepted
  beside it.
- `FormDialog`, `validate_entry_name`, `delete_confirmation` live in `actions.rs` and are
  reused for the local dialogs; `on_click_entity` (formerly `on_click_panel`) is generic so
  both the panel and the pane build menus with it.
- Persistence writer: `schedule_save_table_state` (1 s debounce, background executor), now
  composed by `SftpPanel::persisted_state`.

## Plan

- [x] `SftpTableState` fields + core/state tests; `persistence.md`.
- [x] `local_pane.rs`: `LocalEntry`, `LocalTableDelegate` (3 columns, sort, name cell shared
  with the remote delegate), `LocalPane` entity (cwd, listing, navigation, path input,
  context menu, mutations).
- [x] `SftpPanel`: `expanded`, `local`, toggle, expanded render (`h_resizable`), download
  routing while expanded, drag types + drop handlers, local refresh after download.
- [x] Tests: local listing/sort, expanded download routing, upload from local selection,
  overwrite confirmation, serde round trip.
- [x] Docs + `ci-local` + GUI walk evidence (with the new `sftp-dev-server` diagnostic).
- [x] Follow-up: toggle moved from the toolbar to `Panel::title_suffix`; `SshClientPanel`
  headers render the hosted panel's suffix; gate rerun.
- [x] Rework: zoom-style expand (`SshClientPanel` zoom sync, Maximize/Minimize toggle,
  no-connection layout keeps the toggle, persisted `local_dir` applied to an already
  expanded pane); HLD, README, `gui-layout.md`, design record §4.15 reconciled; gate rerun.

## Decisions

- None (the HLD records the two settled questions).

## Verification Plan

- `cargo test -p oneterm-sftp-ui -p oneterm-core -p oneterm-state`
- `pwsh scripts/ci-local.ps1`
- Manual GUI walk on Windows (`fast-dev` build): expand, browse, upload, download, drag.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [ ] Integration proof
- [x] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/sftp-dual-pane`, 2026-09-11):

- `rtk proxy cargo test -p oneterm-sftp-ui -p oneterm-core -p oneterm-state` — 49 / 51 / 37
  tests passed (sftp-ui gained 6, core 1).
- `rtk proxy cargo clippy --workspace --all-targets -- -D warnings` — clean (two workspace
  test initialisers of `SftpTableState` updated with `..Default::default()`).
- `rtk proxy pwsh scripts/ci-local.ps1` — first run failed in
  `oneterm-local-shell::session_tests::selection_text_and_clear` (a ConPTY echo test with a
  2 s wait, crate untouched by this packet); the crate passed twice in isolation, and the
  rerun of the whole gate printed `ci-local: all checks passed` with no failed test.
- GUI: `evidence/US-0062-gui-walk.md` with seven screenshots, driven against
  `sftp-dev-server` on `127.0.0.1:2222`.
- Rework (2026-09-11): `cargo test -p oneterm-sftp-ui -p oneterm-app` green; clippy clean;
  `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`; GUI: five
  `US-0062-rework-*` captures (zoomed expand, docked collapse, expanded-at-startup without a
  connection, restored zoom with the persisted local dir, collapse without a connection).
  Two defects found by the walk were fixed before the gate (panic when zooming from inside
  the panel update; home directory shown instead of the persisted `local_dir` after a
  restored zoom).

Deviations from the HLD: none. The LLD's "rename refuses an existing target" and the
Windows `\` rejection were implemented as designed; the `\` rule now also applies to the
remote dialogs (a `\` in a remote name was already mangled by `RemotePath`).

Gaps: the shell does not record a zoom restored at startup in `zoomed_panel` when the
window is closed without any other layout change (observed, pre-existing, not changed:
the browser still re-expands from `sftp_table_state.expanded`). Local Rename and typed drive
paths were not walked in the GUI (unit-level coverage of
the validator; the rename flow is the shared `FormDialog`). No real remote host: the walk used
the loopback dev server, whose SFTP surface is the subset OneTerm calls (realpath, stat,
lstat, opendir/readdir, open/read/write/close, mkdir, remove, rmdir, rename, setstat).

## Handoff

Rework implemented on `feat/sftp-dual-pane`, gate green, not committed.
