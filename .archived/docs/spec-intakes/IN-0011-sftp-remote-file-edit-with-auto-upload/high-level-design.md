# High-Level Design: SFTP remote file edit with auto-upload

Intake: IN-0011
Lane: high_risk
Date: 2026-08-19

## Idea

Add an "Edit" workflow to the SFTP browser that turns a remote file into a
short round-trip through a local editor:

1. The user picks **Edit** from a file's context menu.
2. If the file exceeds a configurable size limit (default 1 MB), a confirmation
   is shown first. It is then downloaded to a managed **temp copy**; its remote
   `modified` time is recorded as the *baseline*.
3. The configured editor opens the temp copy — the **OS default application**
   by default, or a **custom command** from a new "SFTP" settings section.
4. A file watcher observes the temp copy. On each save, a dialog asks whether to
   upload (Yes / No), with an **"always upload for this file"** checkbox scoped
   to the current edit session only.
5. Before overwriting, the workflow re-`stat`s the remote file. If the remote
   `modified` time differs from the baseline, it **warns about a conflict**
   before uploading.
6. On upload success, the baseline is refreshed to the new remote mtime so the
   next save compares against the latest state.
7. When the edit session ends (file no longer tracked, panel/session closed,
   app exit) the watcher stops and the temp copy is removed.

The workflow is additive: it reuses the existing `SftpBackend::{stat, upload,
download}` and the transfer queue, and it does not change the transfer pipeline,
`RemotePath`, host-key policy, or the dock schema. The launcher (`open` crate +
custom argv), the `notify` watcher, and the `crates/core` launcher home are
fixed in **DEC-0004**.

## Diagram

```text
                        SFTP browser (crates/sftp-ui)
  context menu "Edit"
        |
        v
  do_edit(entry)
        |            SftpBackend::download (temp copy)          ssh crate
        +--------------------------------------------------------> [remote file]
        |            <- FileEntry.modified = baseline
        v
  EditSession {                          EditorLauncher (core)
    remote_path, temp_path,      launch(editor_cfg, temp_path)
    baseline_mtime,        ------------------------------------> OS default app
    always_upload: false,                                        or custom argv
    watcher                                                      (spawned process)
  }
        |
        |  notify watcher: temp_path modified (debounced)
        v
  on_temp_saved(session_id)
        |
        +-- always_upload? --yes--> upload path
        |
       no
        v
   "File saved. Upload to remote?" dialog  [Yes] [No] [x] always upload this file
        |                                            |
       Yes / always                                  set session.always_upload
        v
  SftpBackend::stat(remote) -> current mtime
        |
        +-- current != baseline? --yes--> conflict warning dialog
        |                                   [Upload anyway] [Cancel]
       no / "Upload anyway"
        v
  SftpBackend::upload(temp_path -> remote)  (reuses transfer queue)
        |
        v
  on success: baseline_mtime = new remote mtime
```

## Data Flow

1. **Trigger.** `build_entry_menu` gains an "Edit" item (files only) bound to a
   new `SftpEdit` action; its `on_click` calls `SftpPanel::do_edit`.
2. **Download.** `do_edit` resolves the selected `FileEntry`, records
   `entry.modified` as the baseline, allocates a temp path under the edit-cache
   directory (`config_dir()/edit-cache/<session-id>/<sanitized-name>`), and
   downloads via `SftpBackend::download` (reusing `run_transfer`). The remote
   file name is preserved so the editor shows the real name / extension.
3. **Launch.** After the download settles, `EditorLauncher::launch` opens the
   temp path with the configured editor:
   - OS-default mode → the platform opener (`start ""` on Windows, `open` on
     macOS, `xdg-open` on Linux, or the `open` crate).
   - Custom mode → the configured argv (`program` + `args`), with the temp path
     appended as the final argument. **No shell string** — argv is passed
     directly so file names cannot inject commands.
4. **Register.** An `EditSession` is stored in a panel-owned registry keyed by a
   session id, holding `remote_path`, `temp_path`, `baseline_mtime`,
   `always_upload = false`, and a `notify` watcher on the temp path.
5. **Watch.** The watcher debounces modify events (editors write in bursts / via
   atomic rename). A debounced event posts `on_temp_saved(session_id)` back to
   the UI thread.
6. **Prompt.** `on_temp_saved`: if `always_upload`, skip to step 7. Otherwise
   show the upload dialog. "No" ends the handling; "Yes" continues; the checkbox
   sets `session.always_upload = true` for this session only.
7. **Conflict check.** `SftpBackend::stat(remote_path)` returns the current
   `modified`. If it differs from `baseline_mtime`, show a conflict warning
   ("the remote file changed since you opened it"); proceed only on "Upload
   anyway".
8. **Upload.** `SftpBackend::upload(temp_path, remote_path)` runs through the
   existing transfer queue. On success, refresh `baseline_mtime` from a fresh
   `stat` (or the value known post-upload) and refresh the listing if the file
   is in the current `cwd`.
9. **Teardown.** Ending an edit session drops the watcher and deletes the temp
   copy. All active sessions are ended on panel disposal, backend switch of that
   session, and app exit; a best-effort sweep removes orphaned edit-cache dirs
   on startup.

## Invariants

- **No shell interpolation for launches.** Editor programs are spawned with an
  explicit argv vector; the temp path is a separate argument. This holds for
  both custom commands and the OS-default opener.
- **Never silently clobber a changed remote file.** An upload only overwrites
  when either the remote mtime equals the recorded baseline or the user
  explicitly chose "Upload anyway".
- **"Always upload" is per edit session**, reset when the session ends; it is
  never persisted and never applies to other files or future sessions.
- **Temp content is confined and cleaned up.** Temp copies live only under the
  dedicated edit-cache directory and are removed when their session ends; no
  edit content is written elsewhere.
- **Additive config compatibility.** The `sftp` group loads from `Default`
  (OS-default editor, 1 MiB edit-size limit) when absent from `terminal.json`;
  no secret is persisted.
- **Backend contract unchanged.** `SftpBackend` is reused as-is; no new trait
  method is introduced by this feature.
- **UI-thread ownership.** The edit-session registry and all dialogs are touched
  only on the UI thread; the watcher hands work back via a channel/`cx.spawn`.

## Detail Design

Detail design is **required for the high-risk lane**. One file per concern under
`low-level-design/`:

- [x] Detail design: required (high-risk)
- Reason: writes remote content to local disk, spawns external processes from
  configurable input, and has an overwrite/conflict safety rule — each needs
  implementation-level review.

Concerns (see `low-level-design/`):

1. `editor-config.md` — the `sftp` config group + settings page + editor
   resolution (OS default vs custom).
2. `editor-launcher.md` — the process/OS-default launcher, argv safety, and its
   home crate.
3. `edit-session-lifecycle.md` — download-to-temp, the watcher + debounce, the
   save/upload prompt, the mtime conflict rule, and temp-file cleanup.
