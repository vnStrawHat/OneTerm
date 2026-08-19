# Work: Watch temp copy and auto-upload on save with conflict guard

ID: US-0023
Intake: IN-0011
Created: 2026-08-19

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [x] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high-risk
- Spec Intake, when required: IN-0011
- Ordering: blocked by US-0022 (needs the edit session + temp copy). Final packet.

## Outcome

While a remote file is being edited, saving it locally is detected by a file
watcher. On a detected save the user is asked whether to upload (Yes / No) with
a per-file "always upload" option; when uploading, the workflow first checks the
remote `modified` time against the baseline and warns before overwriting a
remotely-changed file. Successful upload updates the baseline. Edit sessions and
their temp copies are cleaned up on end/close/exit.

## Scope

- [ ] In scope: `notify` watcher on the temp copy + debounce; the save/upload
  prompt with the session-scoped "always upload" checkbox; the remote-mtime
  conflict warning; re-upload via the transfer queue; baseline refresh;
  `end_edit_session` / `end_all_edit_sessions` teardown; startup + exit
  edit-cache sweep.
- [ ] Out of scope: multi-file/folder edit, diff/merge of conflicts, editing
  unsaved buffers (only saved files are reacted to).

## Acceptance

- [ ] Saving the temp copy triggers exactly one upload prompt per save burst
  (debounced).
- [ ] "No" uploads nothing; the next save prompts again. "Yes" uploads. The
  "always upload this file" checkbox suppresses further prompts for that session
  only — a second file / new session still prompts.
- [ ] When the remote mtime differs from the baseline, a conflict warning is
  shown and upload proceeds only on "Upload anyway"; an unknown mtime also warns.
- [ ] A successful upload refreshes the baseline so an immediate second save does
  not falsely report a conflict; the listing refreshes when the file is in `cwd`.
- [ ] Ending a session drops the watcher and deletes the temp copy; app exit and
  next startup remove leftover edit-cache dirs.
- [ ] `cargo test -p oneterm-sftp-ui` passes; `cargo clippy` clean.

## Documentation

### Owning Docs Reviewed

- `.../low-level-design/edit-session-lifecycle.md` — steps 3–6 owned here.
- `crates/sftp-ui/src/transfer.rs` — upload path + `retain_active` teardown model.
- `crates/state/src/form_dialog.rs` — dialog pattern for the prompt/checkbox.
- `docs/sftp-browser-design.md` — edit-workflow section (finalized here).
- `docs/PROJECT.md` — persistence/temp list (records the edit-cache location).

### Documentation Action

- Update required: complete the edit-workflow section in
  `docs/sftp-browser-design.md` (watcher → prompt → conflict → upload) and note
  the `edit-cache` temp directory in `docs/PROJECT.md`.

Reason: this finalizes a new user-facing workflow and introduces a new local
data-at-rest location that PROJECT.md tracks.

### Reconciliation

Before completion, confirm the sftp-browser-design edit section matches the
shipped behavior and PROJECT.md lists the edit-cache directory.

## Context

New dependency `notify` (pinned in the root `Cargo.toml`). The watcher callback
runs off the UI thread and must forward events over `async_channel` to a
`cx.spawn` loop that calls `on_temp_saved` on the UI thread. Reuse the existing
transfer queue for the re-upload. All `EditSession` / registry access is
UI-thread only (ARCH-31 grouping + ARCH-40 store rules).

## Plan

- [ ] Add the `notify` watcher + debounce; forward to `on_temp_saved`.
- [ ] Build the save/upload prompt with the session-scoped "always" checkbox.
- [ ] Add the `stat`-based conflict check + warning dialog.
- [ ] Upload via the transfer queue; refresh baseline + listing on success.
- [ ] Implement session teardown + startup/exit edit-cache sweep.
- [ ] Add unit tests (conflict decision, always-upload scoping, teardown,
  debounce).

## Decisions

DEC-0004 — the `notify` watcher (background thread → UI thread via
`async_channel`) is inherited here. Conflict policy (warn on remote-mtime change)
and "always upload" being session-scoped are fixed by IN-0011.

## Verification Plan

Unit (`FakeSftpBackend`): upload when mtime == baseline; warn + gate when it
differs or is unknown; `always_upload` scoping; baseline refresh; teardown drops
watcher + temp; debounce coalescing. Manual (Windows): real edit → save → upload
→ induced conflict → cleanup. Command: `cargo test -p oneterm-sftp-ui` +
`pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Implemented 2026-08-19 (together with US-0022 in `crates/sftp-ui/src/edit.rs`).

- Added the `notify` crate (root `Cargo.toml` + `crates/sftp-ui`). A
  `notify::recommended_watcher` on the temp session directory forwards write
  events for the temp file over an `async_channel`; a `cx.spawn_in` loop
  coalesces bursts with a 400 ms debounce and calls `on_temp_saved`.
- Save prompt: `FormDialog` with an "Always upload this file while editing"
  checkbox whose flag is stored on the `EditSession` (session-scoped, never
  persisted). "Upload"/always → conflict check; "Don't upload" keeps the temp
  copy and re-prompts on the next save.
- Conflict guard: `begin_conflict_check_and_upload_ctx` stats the remote and
  compares mtime to the recorded baseline (`is_conflict`: unequal or unknown →
  warn); the warning dialog (`warn_conflict_then_upload`) gates the upload behind
  "Upload anyway". Upload runs through the transfer queue; `finish_upload`
  refreshes the baseline from a fresh stat, refreshes the listing when the file
  is in the current cwd, and re-runs a save that arrived mid-upload
  (`pending_save`, at most one upload in flight per file).
- Teardown: `end_edit_session` / `end_edit_sessions_for_backend` (called from
  `sync_from_app_state` on backend switch) / the `Drop` impl remove the watcher
  and the temp copy; `sweep_edit_cache` in the feature `init` clears leftovers
  from a previous run.
- Unit tests (`edit::tests`): conflict decision (equal / changed / unknown
  mtimes), editor-choice mapping, and file-name sanitisation.

Commands:

- `cargo test -p oneterm-sftp-ui` → 38 passed (harness verify).
- `cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D
  warnings` → clean.

Gaps: the watcher → debounce → prompt → conflict → upload path is not exercised
end-to-end by an automated test (it needs a real filesystem watcher event + an
open window for the dialogs); the decision logic is unit-tested and the flow
needs a manual Windows pass (edit → save → upload; induce a conflict by touching
the remote file mid-edit; verify temp cleanup on close).

## Handoff

Feature complete pending the manual Windows E2E pass noted above.

### Acceptance rework 2026-08-19 (reopened)

Symptom: edit a file → save → no dialog, no upload; remote unchanged.

Root cause: the watcher forwarded an event only when a reported path was
`== temp_path`. `temp_path` is built from `config_dir()`, which is the relative
`target/` in debug builds, while `notify` reports absolute, canonicalised paths
— so the equality never held and no save event ever reached `on_temp_saved`.

Fix (`crates/sftp-ui/src/edit.rs`):

- Extracted `event_is_save(event, target_name)` matching by **file name**
  (`Path::file_name`) instead of full path, ignoring `Access` (read/metadata)
  events. Name-matching also covers editors that save via an atomic
  sibling-write + rename.
- Canonicalised `watch_dir` before `watcher.watch(...)` so the registration does
  not depend on the process CWD, and added an info log of the watched dir plus a
  debug log of each forwarded event kind.
- Added regression test
  `watcher_matches_the_temp_file_by_name_regardless_of_event_path_shape`
  (absolute event path vs. relative-registered temp file; rename-create; a
  different file; a pure access read).

`cargo test -p oneterm-sftp-ui` → 39 passed; `cargo clippy -p oneterm-sftp-ui
--all-targets -- -D warnings` clean. Still needs the manual Windows E2E pass to
confirm a real editor save now raises the upload dialog.

### Acceptance rework 2026-08-19 (round 2 — cleanup + cache identity)

Feedback: (1) save→dialog now works; (2) temp cache was not cleaned when the
user closed the editor / stopped editing; (3) the `edit-cache/<edit-id>` folder
was not clearly scoped for multiple tabs / multiple OneTerm instances.

Decisions (from the user): do not track editor exit (unreliable for OS-default /
GUI editors) — clean up when the SSH/SFTP session closes. The SFTP session id is
enough to keep tabs/instances independent; adding ssh host/port/user to the path
is a nice-to-have, skipped as it needs an `SftpBackend` contract change.

Changes (`crates/sftp-ui/src/edit.rs`, `panel.rs`):

- Temp path is now `edit-cache/<pid>/<edit-session-id>/<name>`
  (`process_cache_root()`). The pid isolates concurrent instances (their
  `EditSessionId` counters both start at 1) and the startup sweep now removes
  only **this process's** subdirectory, never another running instance's live
  files.
- Replaced `end_edit_sessions_for_backend` (which ended sessions on any tab
  switch) with `reap_dead_edit_sessions`, which ends only sessions whose
  `SftpBackend::alive()` is false. It runs on the 500 ms poll tick and on every
  active-tab change, so a closed connection's temp copies are cleaned up while
  the app keeps running, but a plain tab switch keeps a live session editable.
- Added test `process_cache_root_is_scoped_to_this_process`.

`cargo test -p oneterm-sftp-ui` → 40 passed; `cargo fmt` + `cargo clippy
--workspace --all-targets -- -D warnings` clean; `cargo build --workspace` ok.
Docs reconciled: LLD `edit-session-lifecycle.md` (teardown + temp-path scheme),
`docs/sftp-browser-design.md` §4.14, `docs/PROJECT.md` (edit-cache/<pid>).

Remaining gap: the temp copy is not removed the instant the *editor* is closed
(no reliable signal); it is cleaned when the SFTP session closes, on panel drop,
on app exit, or by the next startup sweep. Manual Windows E2E of the full
save→upload→conflict flow and the session-close cleanup still to be run.

### Acceptance rework 2026-08-19 (round 3 — pid folder never reclaimed)

Feedback: (1) closing OneTerm while a file is being edited left the
`edit-cache/<pid>/…` behind; (2) closing the SSH/SFTP session removed the
`<edit-session-id>` folder but the `<pid>` folder always remained — including
across full app restarts.

Root cause: (a) `cleanup_temp` removed only the `<edit-session-id>` directory,
never the parent `<pid>` directory, so an empty `<pid>` folder lingered; and (b)
the startup sweep removed **only the current process's own** `<pid>` folder —
which does not exist yet at startup — so a folder left by a *killed* previous
run (with a different pid, where `Drop` never ran) was never reclaimed.

Fix (`crates/sftp-ui/src/edit.rs`):

- `cleanup_temp` now also calls `prune_empty_process_root`, which `remove_dir`s
  `edit-cache/<pid>/` **only when empty** — the folder disappears as soon as its
  last session ends, and a still-active session's folder is untouched.
- `sweep_edit_cache` is now pid-aware: it scans **every** `edit-cache/<pid>/`
  and, using a `sysinfo` process snapshot, reclaims directories whose owning pid
  is no longer alive (killed prior run) plus any leftover reusing this process's
  own pid, while keeping directories owned by another **still-running** OneTerm
  instance. This is what finally removes the `<pid>` folder after the app is
  killed without a clean `Drop`.
- Added `sysinfo` to `crates/sftp-ui/Cargo.toml`.
- Added helper `should_reclaim_dir(pid, current, pid_is_live)` + test
  `sweep_reclaims_dead_and_same_pid_dirs_but_keeps_other_live_instances`.

`cargo test -p oneterm-sftp-ui` → 41 passed; `cargo fmt` + `cargo clippy
-p oneterm-sftp-ui --all-targets -- -D warnings` clean; `cargo build --workspace`
ok. Docs reconciled: LLD `edit-session-lifecycle.md` (teardown + temp-path
scheme), `docs/sftp-browser-design.md` §4.14, `docs/PROJECT.md`. Manual Windows
E2E: (a) edit → close OneTerm → `<pid>` folder gone; (b) edit → close SSH
session → both folders gone; (c) kill OneTerm mid-edit → relaunch → stale `<pid>`
folder reclaimed; (d) two instances editing concurrently → neither sweep touches
the other's live files — still to be run.

### Acceptance rework 2026-08-19 (round 4 — dialog on open, before any edit)

Feedback: selecting "Edit" popped the "Upload change" dialog immediately, before
any real edit.

Root cause: `event_is_save` only filtered by file name + `Access` events. When
an editor opens the file it touches metadata/attributes, which on Windows
`ReadDirectoryChangesW` reports as a generic `Modify` — indistinguishable from a
content write by event kind — so the watcher fired and `on_temp_saved` prompted
before the user changed anything.

Fix (`crates/sftp-ui/src/edit.rs`):

- Added `temp_signature(path) -> Option<(mtime, len)>` and an `EditSession`
  field `last_temp_sig`, seeded from the freshly downloaded copy.
- `on_temp_saved` now confirms the bytes actually changed: it compares a fresh
  fingerprint against `last_temp_sig` and ignores the event when they are equal
  (editor open / metadata touch / read), recording the new fingerprint only on
  a real change. This keeps `event_is_save` as a cheap first filter while the
  fingerprint is the authoritative save signal.
- Added test `temp_signature_changes_on_content_edit_but_not_on_a_read`.

`cargo test -p oneterm-sftp-ui` → 42 passed; `cargo fmt` + `cargo clippy
-p oneterm-sftp-ui --all-targets -- -D warnings` clean; `cargo build --workspace`
ok. Docs reconciled: LLD `edit-session-lifecycle.md` (Step 3 content guard +
Step 4). Manual Windows E2E: open a file for editing and confirm no dialog
appears until an actual save — still to be run.
