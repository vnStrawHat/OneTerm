# Work: Context-menu Edit: download to temp and launch editor

ID: US-0022
Intake: IN-0011
Created: 2026-08-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk
- Spec Intake, when required: IN-0011
- Ordering: blocked by US-0021 (needs the editor config). Blocks US-0023.

## Outcome

The SFTP file context menu has an "Edit" item (files only). Choosing it
downloads the remote file to a managed local temp copy (surfaced in the transfer
queue), records the remote `modified` time as a baseline, and launches the
configured editor (OS default or custom command) on the temp copy. A failed
launch cleans up the temp copy and notifies the user, leaving no orphaned state.

## Scope

- [ ] In scope: `SftpEdit` action; the file-branch "Edit" menu item;
  `SftpPanel::do_edit` (size-gate + download-to-temp + launch); the
  `EditorChoice` mapping from `EditorConfig`; the `oneterm_core::launch_editor`
  helper using the **`open` crate** (DEC-0004); temp-path allocation under
  `config_dir()/edit-cache`; the `EditSession` struct + registry scaffold
  (created here, driven in US-0023).
- [ ] Out of scope: the file watcher, the save/upload prompt, the mtime conflict
  check, and re-upload (US-0023).

## Acceptance

- [ ] The context menu shows "Edit" for files and not for folders.
- [ ] A file larger than the configured `edit_max_file_size` (and limit != 0)
  shows a confirmation before any download; Cancel starts nothing; `0` disables
  the gate.
- [ ] Choosing Edit downloads the file to `edit-cache/<session>/<name>` and the
  transfer appears in the queue.
- [ ] After download, the configured editor opens the temp copy: OS default when
  mode = OS default (or Custom with an empty program); the custom argv otherwise,
  with the temp path passed as a separate argument.
- [ ] A launch failure (e.g. bogus custom program) notifies the user and deletes
  the temp copy; no `EditSession` is registered.
- [ ] `cargo test -p oneterm-core -p oneterm-sftp-ui` passes; `cargo clippy` clean.

## Documentation

### Owning Docs Reviewed

- `.../low-level-design/editor-launcher.md` — launcher API, argv safety, crate home.
- `.../low-level-design/edit-session-lifecycle.md` — steps 1–2 (download + launch),
  temp-path scheme, `EditSession` shape.
- `crates/sftp-ui/src/transfer.rs` (`download_to`, `run_transfer`) — reused
  download path.
- `crates/sftp-ui/src/table_delegate_menu.rs` — where the menu item is added.
- `docs/sftp-browser-design.md` §4.6 — file-op table (adds the Edit row).

### Documentation Action

- Update required: add an "Edit" row to the file-operations table in
  `docs/sftp-browser-design.md` §4.6 and a short edit-workflow note.

Reason: Edit is a new user-facing file operation the SFTP design doc enumerates.

### Reconciliation

Before completion, confirm the sftp-browser-design file-op table lists Edit and
the launcher helper matches editor-launcher.md.

## Context

Reuse `SftpBackend::download` through the existing transfer helpers so
cancellation and queue display work unchanged. `launch_editor` lives in
`crates/core` and takes an `EditorChoice` (no `core → settings` dependency);
`crates/sftp-ui` maps `EditorConfig` → `EditorChoice`. The OS-default path uses
the **`open` crate**; both `open` and `notify` are pinned once in the root
`Cargo.toml` (DEC-0004). The size gate reads `config.sftp.edit_max_file_size`.

## Plan

- [ ] Add `SftpEdit` to `crates/actions`.
- [ ] Add `oneterm_core::editor_launcher` (`EditorChoice`, `launch_editor`).
- [ ] Add the "Edit" menu item (file branch) + `SftpPanel::do_edit`.
- [ ] Add the size-gate confirmation before download.
- [ ] Allocate the temp path, download via the transfer path, launch on success.
- [ ] Add the `EditSession` struct + registry field (populated here; watcher in
  US-0023).

## Decisions

DEC-0004 — `open` crate for OS-default launch and `crates/core` as the launcher
home are inherited here. No new decision is owned by this packet.

## Verification Plan

Unit: launcher resolution (`Custom` empty program → OS default; argv builder keeps
the path as one element). Integration: `do_edit` with `FakeSftpBackend` downloads
to temp and registers no session on launch failure. Command:
`cargo test -p oneterm-core -p oneterm-sftp-ui`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Implemented 2026-08-19.

- Added `open` crate (root `Cargo.toml` + `crates/core`); `oneterm_core::
  editor_launcher` (`EditorChoice`, `launch_editor`) opens OS-default via
  `open::that_detached` and custom editors via `std::process::Command` with an
  explicit argv (path as a separate final argument). 4 unit tests.
- Added `SftpEdit` action; "Edit" item in the file branch of the context menu
  (`table_delegate_menu.rs`) and the `SftpEdit` on_action handler in `render.rs`.
- Added `crates/sftp-ui/src/edit.rs`: `SftpPanel::do_edit` (size gate reading
  `sftp.edit_max_file_size`; download to `config_dir()/edit-cache/<id>/<name>`
  via the transfer queue; `start_edit`/`register_edit_session`), the
  `EditSession` registry field + accessors on `SftpPanel`, and the
  `editor_choice`/`sanitize_file_name` helpers. `oneterm-settings` added to the
  crate's deps to read the editor config.
- Docs: `docs/sftp-browser-design.md` §4.6 gained the "Edit" row and a new §4.14
  edit-workflow section; `docs/PROJECT.md` persistence list gained `edit-cache/`.

Commands:

- `cargo test -p oneterm-core -p oneterm-sftp-ui` → 43 + 38 passed (harness verify).
- `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D
  warnings` → clean.

Gaps: launching a real editor + OS-default association is not exercised by an
automated test (needs a desktop session); covered by the argv/choice unit tests
and a manual Windows pass. The watcher-driven save/upload behavior is US-0023.

## Handoff

US-0023 builds on the `EditSession` registry and the watcher/save loop added
here (they were implemented together in `edit.rs`).
