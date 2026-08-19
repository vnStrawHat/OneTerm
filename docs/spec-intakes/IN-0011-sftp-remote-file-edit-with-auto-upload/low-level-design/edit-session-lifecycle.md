# Low-Level Design: Edit session lifecycle

Intake: IN-0011
HLD: ../high-level-design.md
Topic: edit-session-lifecycle
Date: 2026-08-19

## Concern

The runtime workflow that ties everything together: download-to-temp, the
`notify` watcher and its debounce, the save/upload prompt (with per-file
"always upload"), the remote-mtime conflict rule, and temp-file cleanup. Owned
by `crates/sftp-ui`.

## Design

### Where it lives

New module `crates/sftp-ui/src/edit.rs`, holding an `EditSession` registry on
`SftpPanel`. It reuses `SftpPanel::sftp()` / `active_key()` and the transfer
helpers already used by `transfer.rs`.

```rust
// crates/sftp-ui/src/edit.rs

/// One in-flight "edit this remote file locally" session.
pub(crate) struct EditSession {
    id: EditSessionId,           // process-local counter
    backend_key: BackendKey,     // SftpSessionId — which SFTP session it belongs to
    remote_path: RemotePath,
    temp_path: PathBuf,
    /// Remote mtime last known to match `temp_path`'s content on the server.
    baseline_mtime: Option<SystemTime>,
    /// Set by the "always upload for this file" checkbox — session-scoped only.
    always_upload: bool,
    /// Kept alive to keep watching; dropping it stops the watcher.
    _watcher: notify::RecommendedWatcher,
}
```

The registry: `edit_sessions: HashMap<EditSessionId, EditSession>` on the panel
(added to the panel's grouped state per ARCH-31). All access is UI-thread only.

### Step 1 — trigger and download

`SftpPanel::do_edit(&mut self, window, cx)` (files only; the menu item is added
only in the file branch of `build_entry_menu`):

1. Resolve the selected `FileEntry`; if it is a directory, ignore (menu item is
   file-only anyway).
2. Read the current editor config (`TerminalConfig::load().sftp`) and map the
   editor to `EditorChoice`.
3. **Size gate.** Let `limit = config.sftp.edit_max_file_size`. If `limit != 0`
   and `entry.size > limit`, show a confirmation dialog ("This file is X, larger
   than the configured edit limit of Y. Open for editing anyway?") with
   `[Open anyway]` / `[Cancel]`. Cancel aborts before any download. `limit == 0`
   skips the gate. This is a warning, not a hard block (per IN-0011 review).
4. Allocate `temp_path = config_dir()/edit-cache/<session-id>/<sanitized name>`.
   Keep the original file name (so the extension drives editor syntax); sanitize
   only path separators.
5. `baseline_mtime = entry.modified`.
6. Download using the same mechanism as `download_to` in `transfer.rs`
   (`SftpBackend::download` + `run_transfer`), so the transfer appears in the
   queue and cancellation works.
7. On download success, go to step 2 (launch). On failure/cancel, delete the
   temp dir and do not register a session.

### Step 2 — launch + register

After the download settles on the UI thread:

1. `launch_editor(&choice, &temp_path)`; on error, notify, delete the temp copy,
   and stop (no session registered).
2. Create a `notify` watcher on `temp_path` (watch the parent dir non-recursive
   if watching a single file is unreliable on the platform; filter events to the
   temp path). Store the `EditSession` in the registry.

### Step 3 — watch + debounce

The `notify` callback runs on the watcher thread. It must not touch gpui state
directly. It forwards debounced events over an `async_channel` whose receiver is
polled by a `cx.spawn` loop bound to the panel; the loop calls
`on_temp_saved(session_id)` on the UI thread.

Debounce: editors save via truncate-write or atomic rename, emitting several
events. Coalesce events per session within a short window (~300–500 ms of
quiescence) before emitting one logical "saved". Ignore events after the session
is removed.

Content guard: `event_is_save` is only a cheap first filter (name match, ignore
`Access`). On Windows `ReadDirectoryChangesW` reports a generic `Modify` for the
metadata/attribute touches an editor makes when it merely *opens* the file, so
the watcher fires before any real edit. `on_temp_saved` therefore confirms the
bytes actually changed by comparing a fresh `temp_signature` — `(mtime, len)` of
the temp copy — against `last_temp_sig` (seeded from the freshly downloaded copy
and refreshed on every real change). An equal fingerprint is not a save and is
ignored, so opening the file never pops the upload dialog on its own.

### Step 4 — save prompt

`SftpPanel::on_temp_saved(session_id, window, cx)`:

1. Look up the session; if gone, ignore.
2. Compare a fresh `temp_signature` against `last_temp_sig`; if equal, the file
   was not really written (editor open / metadata touch) — ignore. Otherwise
   record the new fingerprint.
3. If `session.always_upload`, jump to step 5.
4. Show a dialog: "‹name› was saved. Upload to the remote host?" with
   `[Upload]` / `[Don't upload]` and a checkbox "Always upload this file while
   editing". Use `oneterm_state::form_dialog` / an alert dialog consistent with
   the existing rename/delete dialogs.
5. Checkbox checked → set `session.always_upload = true` (session-scoped; never
   persisted).
6. "Upload" / "always" → step 5. "Don't upload" → return (the temp copy stays;
   the next save re-prompts).

### Step 5 — conflict check + upload

1. `SftpBackend::stat(remote_path)` → `current_mtime = FileEntry.modified`.
2. If `current_mtime != baseline_mtime` (and both are `Some`, or one flipped to
   `None`), show a **conflict warning**: "The remote file has changed since you
   started editing. Uploading will overwrite those changes." → `[Upload anyway]`
   / `[Cancel]`. Cancel returns without uploading.
   - If either mtime is unknown (`None`) treat as "cannot prove unchanged" and
     warn, to stay on the safe side of the never-silently-clobber invariant.
3. `SftpBackend::upload(temp_path, remote_path)` via the transfer queue.
4. On success: set `baseline_mtime` from a fresh `stat` (or the post-upload
   value) so subsequent saves compare against the new state; refresh the listing
   if `remote_path`'s parent is the current `cwd`.
5. On failure: notify; keep the session (the user can save again / retry).

### Step 6 — teardown

End a session (`end_edit_session(id)`): drop the watcher (`_watcher`), delete the
temp copy, remove from the registry. Triggers:

- **SFTP session closed.** `reap_dead_edit_sessions` ends every session whose
  `SftpBackend::alive()` is false. It runs on the 500 ms poll tick and on every
  active-tab change, so a closed SSH/SFTP connection's temp copies are removed
  while the app keeps running. A plain tab switch does **not** end a live
  session's edit sessions — the user can switch back and keep editing.
- **Panel disposal / app exit.** `SftpPanel`'s `Drop` cleans up every remaining
  session's temp copy, then prunes the now-empty `edit-cache/<pid>/` directory.
- **Session cleanup empties the pid folder.** Each `cleanup_temp` also calls
  `prune_empty_process_root`, which `remove_dir`s `edit-cache/<pid>/` **only when
  empty** — so the per-process folder disappears once its last session ends,
  while a still-active session's folder is left untouched.
- **Startup sweep.** `sweep_edit_cache` scans every `edit-cache/<pid>/` and
  reclaims those whose owning pid is no longer a live process (a previous run
  that exited or crashed before its `Drop`/session cleanup ran), plus any
  leftover directory reusing this process's own pid. Directories owned by
  **another still-running OneTerm instance** are kept. This is what finally
  removes the `<pid>` folder left behind when the whole app is killed without a
  clean `Drop` (see the temp-path scheme below).

Editor-exit tracking is deliberately **not** used: the OS-default opener and most
GUI editors return immediately after launch and give no usable process handle,
so there is no reliable "editor closed" signal. Cleanup is therefore tied to the
SFTP session lifecycle plus panel/app teardown and the startup sweep.

Cleanup is best-effort (a locked temp file on Windows may resist deletion); a
failed delete is logged, not surfaced, and retried by the startup sweep.

### Temp-path scheme

Temp copies live under `config_dir()/edit-cache/<pid>/<edit-session-id>/<name>`:

- `<pid>` (`std::process::id()`) isolates concurrent OneTerm instances — two
  instances never share a directory (their `EditSessionId` counters both start
  at 1). The startup sweep is pid-aware: it removes only directories whose pid
  is dead (or this process's own leftover), never another running instance's
  live files. When a process exits cleanly the folder is pruned as soon as it is
  empty (`prune_empty_process_root`); if it is killed, the next launch's sweep
  reclaims it because the pid is no longer live.
- `<edit-session-id>` is the process-local `EditSessionId`, unique per edit
  session (and thus per open file) within the process. Combined with the SFTP
  `session_id` recorded on the `EditSession`, this is enough to keep multiple
  tabs and multiple instances independent, so the SSH host/port/user are not
  embedded in the path (they would need a `SftpBackend` contract change).

## Interfaces

```rust
// crates/sftp-ui/src/edit.rs (all pub(crate), on SftpPanel)
impl SftpPanel {
    pub(crate) fn do_edit(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn on_temp_saved(&mut self, id: EditSessionId, window: &mut Window, cx: &mut Context<Self>);
    fn end_edit_session(&mut self, id: EditSessionId, cx: &mut Context<Self>);
    fn end_all_edit_sessions(&mut self, cx: &mut Context<Self>);
}

// crates/actions/src/lib.rs — add to the SFTP actions! block:
//   SftpEdit,

// crates/sftp-ui/src/table_delegate_menu.rs — file branch of build_entry_menu:
//   PopupMenuItem::new("Edit").action(Box::new(SftpEdit))
//       .on_click(on_click_panel(panel.clone(), SftpPanel::do_edit))
```

## Edge Cases and Failure Modes

- [ ] Same file opened for edit twice → reuse the existing session (focus/relaunch
  editor) rather than creating a second temp + watcher.
- [ ] File exceeds `edit_max_file_size` → confirmation gate before download;
  Cancel starts nothing. `edit_max_file_size == 0` disables the gate.
- [ ] Editor writes via atomic rename (temp file replaced) → watch the parent
  directory and match by file name so the watch survives the replace.
- [ ] User deletes the temp file externally → treat as session end; stop watching.
- [ ] Remote file deleted before upload → `stat` errors; surface it and offer to
  upload as a new file or cancel (v1: notify + cancel, keep temp).
- [ ] Backend/session closed while editing → end its sessions; a queued save that
  arrives after has no backend and is dropped with a notification.
- [ ] Rapid successive saves → debounce coalesces; while an upload is in flight,
  a new save queues one follow-up upload after it settles (no overlapping
  uploads to the same remote path).
- [ ] App exit with unsaved-in-editor changes → out of scope; we only react to
  saved files. Temp cleanup still runs.

## Verification

- [ ] Unit (with `FakeSftpBackend`): baseline recorded on open; upload proceeds
  when `stat` mtime == baseline; conflict path taken when mtime differs; unknown
  mtime warns; `always_upload` skips the prompt for that session only and does
  not affect a second session/file.
- [ ] Unit: `end_edit_session` drops the watcher and removes the temp file;
  `end_all_edit_sessions` clears the registry.
- [ ] Unit: debounce coalesces a burst of modify events into one `on_temp_saved`.
- [ ] `cargo test -p oneterm-sftp-ui`.
- [ ] Manual (Windows): edit → save → upload prompt → conflict warning by
  touching the remote file mid-edit; verify temp cleanup after close.
