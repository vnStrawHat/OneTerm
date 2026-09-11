# Low-Level Design: Local-side mutations and overwrite guards

Intake: IN-0025
HLD: ../high-level-design.md
Topic: local-mutations
Date: 2026-09-11

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

Every operation the Local pane performs that can destroy or replace data on the user's disk:
Delete (file or recursive folder), Rename, New Folder, and the download target while expanded.

## Design

- Paths are always built from the pane's current `cwd: PathBuf` (an absolute directory that
  came from a successful listing) joined with a name validated by `validate_entry_name`
  (non-empty, no `/`, not `.`/`..`). On Windows the validator also rejects `\` so a typed name
  cannot escape the directory; the check lives in one place and is shared with the remote
  dialogs.
- Delete uses the selected row's `path` (captured from the listing, not re-derived from the
  index at confirm time), `std::fs::remove_file` for files and `std::fs::remove_dir_all` for
  folders. Symlinked folders are removed with `remove_dir` / `remove_file` semantics by the
  standard library (`remove_dir_all` does not follow the root symlink on either platform).
- Rename is `std::fs::rename(from, cwd.join(new_name))` and refuses when the target already
  exists (checked in the same background task, right before the rename) so a rename cannot
  silently replace a sibling.
- New Folder is `std::fs::create_dir(cwd.join(name))` — not `create_dir_all`, so a name that
  already exists errors instead of being reported as created.
- Download while expanded targets `cwd.join(entry.name)`. The spawned task checks
  `Path::exists` on the background executor; when true it opens the alert dialog
  "Replace <name>?" and only the confirm button continues into `download_to`. The check is
  skipped only when the user has already confirmed for that exact path in that flow.
- All `std::fs` calls run inside `cx.background_executor().spawn(..)`; the UI thread only
  builds the owned path and applies the result (notification + local refresh).
- Confirmations reuse the existing wording helper `delete_confirmation(name, is_dir)` so the
  recursive nature of a folder delete is stated.

## Interfaces

```rust
// crates/sftp-ui/src/local_pane.rs
pub(crate) struct LocalPane { cwd: PathBuf, table: Entity<TableState<LocalTableDelegate>>, .. }
impl LocalPane {
    pub(crate) fn cwd(&self) -> &Path;
    pub(crate) fn selected_entry(&self, cx: &App) -> Option<LocalEntry>;
    pub(crate) fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>);
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>);
    pub(crate) fn do_new_folder / do_rename / do_delete(&mut self, window, cx);
}
/// Runs `op` on the background executor, notifies, refreshes the pane.
fn run_local_mutation(pane, operation, success, failure, op: impl FnOnce() -> io::Result<()> + Send, window, cx)
```

## Edge Cases and Failure Modes

- [ ] Delete of a path that vanished → `io::Error` shown as "Delete failed: …", list refreshed.
- [ ] Rename onto an existing name → refused with "already exists", dialog stays open.
- [ ] New Folder with an existing name → `AlreadyExists` error shown, nothing replaced.
- [ ] Download onto an existing local file → confirmation; Cancel starts no transfer.
- [ ] Download onto an existing local *folder* of the same name → confirmation names the
  folder; the backend's download then writes into/over it as it does today from the save
  dialog path.
- [ ] Typed name with a path separator → rejected before any filesystem call.

## Verification

- [ ] `cargo test -p oneterm-sftp-ui`: `validate_entry_name` rejects `\` on Windows;
  download while expanded requests `local_cwd/name` with no save dialog; an existing target
  is not downloaded until confirmed (test drives the confirm button off).
- [ ] Manual GUI walk: delete a temp folder, rename onto an existing name, download over an
  existing file.
