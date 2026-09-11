# High-Level Design: SFTP dual-pane (Local + Remote)

Intake: IN-0025
Lane: high_risk
Date: 2026-09-11

## Idea

`SftpPanel` keeps its remote machinery untouched and gains an `expanded` flag plus one
`LocalPane` entity. Collapsed renders exactly today's layout inside the right dock. Expanded
behaves like zooming a terminal tab (owner acceptance, 2026-09-11): the hosting
`SshClientPanel` zooms its dock node so the browser fills the whole workspace, hides the
Session section, and renders an `h_resizable` split — `LocalPane` left, the existing remote
toolbar + table right — above the shared transfer queue. Collapsing (the same toggle, or any
other zoom-out such as `ToggleZoom` or zooming another group) docks it again. The local side
is a plain `std::fs` browser (listing on the background executor) with its own `DataTable`;
nothing on the local side goes through `SftpBackend`, and nothing on the remote side learns
about local paths beyond what upload/download already take (`PathBuf`). Cross-pane transfers
reuse the existing `do_upload_paths` and `download_to`.

## Diagram

```text
 SshClientPanel (right-dock node, zoomable) ── zoom in/out ⇄ SftpExpandedChanged
 │   expanded ⇒ DockArea::set_zoomed_in(node) + render only the SFTP section
 │   Panel::set_zoomed(false) (any zoom-out) ⇒ sftp.set_expanded(false)
 │
 SftpPanel ──────────────────────────────────────────────────────────────
 │ expanded: bool  (persisted: docks.json sftp_table_state.expanded; the zoom
 │                  itself persists as docks.json zoomed_panel = ssh_client_panel)
 │ local: Entity<LocalPane> ── cwd: PathBuf (persisted: ...local_dir)
 │                          ── TableState<LocalTableDelegate> (entries, sort, selected)
 │                          ── path_input, generation-guarded std::fs listing
 │ browser / table / transfers / follow  (remote, unchanged, per-backend store)
 │
 │  Local ──Upload (button / dbl-click / drag row→remote list)──▶ do_upload_paths([path])
 │  Remote ──Download (menu / drag row→local list)────────────▶ download_to(.., local_cwd/name)
 │            (expanded only: no save dialog; confirm when the target exists)
 └──────────────────────────────────────────────────────────────────────
```

## UI Wireframe

Collapsed (unchanged apart from the toggle `[⤢]` at the trailing end of the section title,
the panel's `title_suffix`, like the terminal tab's zoom button):

```text
+-- SFTP Browser --------------------------------------- | [⤢] +
| [ /home/user            ] [<] [⟳sync] [⟳] [⋮]              |
| Name            | Date Modified   | Permissions | Size ...  |
| 📁 src          | 2026-09-01 ...  | drwxr-xr-x  |           |
| 📄 main.rs      | ...             | -rw-r--r--  | 1.2 KB    |
+------------------------------------------------------------+
| Transfers  1 active, 0 done                        [Clear]  |
+------------------------------------------------------------+
```

Expanded (`[⊟]` in the title collapses again): the zoomed right-dock node fills the
workspace, like a zoomed terminal tab — the center tabs and the Session section are not shown.

```text
+-- OneTerm ------------------------------------------------------------------------------+
| SFTP Browser                                                                     | [⊟]  |
| Local [ C:\Users\me\proj  ] [<] [→] [⟳] [⋮] ║ Remote [ /home/user ] [<] [⟳sync] [⟳] [⋮]     |
| Name          | Date Modified | Size ║ Name        | Date Modified | Perm | Size          |
| 📁 docs       | ...           |      ║ 📁 src      | ...           | drwx |               |
| 📄 a.txt      | ...           | 3 KB ║ 📄 main.rs  | ...           | -rw- | 1.2 KB        |
|   (drag a.txt →)              ↑ [Upload]  [Download] ↓   (← drag main.rs)              |
+------------------------------------------------------------------------------------------+
| Transfers  1 active, 0 done                                                    [Clear]   |
+------------------------------------------------------------------------------------------+
```

Local `[⋮]` menu: New Folder, Upload, Rename, Delete, Refresh. Local row context menu: Open
(folder) / Upload (file), Rename, Delete, separator, New Folder, Refresh. The remote menus are
unchanged.

## Data Flow

1. Toggle: the button at the trailing end of the "SFTP Browser" title — `SftpPanel`'s
   `Panel::title_suffix`, drawn by `SshClientPanel`'s section header inside the same framed
   control group the terminal tab bar uses for its trailing buttons (full height, left
   border, tab-bar padding); Maximize / Minimize icons as on a terminal tab — flips
   `SftpPanel.expanded`, schedules the debounced `docks.json` save, loads the local cwd on
   the first expand (persisted `local_dir`, else the home directory, else `.`), and emits
   `SftpExpandedChanged`. `SshClientPanel` answers by zooming its dock node in
   (`DockArea::set_zoomed_in`) or out (only when that node is the zoomed one) and by
   rendering only the SFTP section while expanded. The base `Panel::set_zoomed` hook on
   `SshClientPanel` closes the loop for every other zoom change (a `ToggleZoom` action,
   zooming another group, the zoom restored from `docks.json` at startup): it sets the
   browser's expanded state to the node's zoom state.
2. Local listing: `LocalPane::load_dir(path)` bumps a generation, runs `std::fs::read_dir`
   + `metadata` on the background executor, and applies the result only when the generation
   still matches; a failure keeps the previous rows under an error banner (same as remote).
   Entries are sorted folders-first by the selected column (Name asc default).
3. Local navigation: double-click a folder / Enter in the path box (directory check on the
   background executor) / back button (`Path::parent`). Each successful load saves
   `local_dir`.
4. Upload: local selection (button, "Upload" menu item, double-click on a file, or a
   `LocalRowDrag` dropped on the remote list) → `SftpPanel::do_upload_paths(vec![path])`,
   which already uploads into the remote cwd and refreshes the remote list. The local list is
   not touched.
5. Download: while expanded, `do_download` skips the save dialog and targets
   `local_cwd.join(entry.name)`; if that path exists the user confirms the overwrite first
   (see LLD). A `RemoteRowDrag` dropped on the local list takes the same path. When the
   transfer settles, the local list refreshes.
6. Local mutations (New Folder / Rename / Delete) reuse the remote dialogs
   (`FormDialog`, `validate_entry_name`, `delete_confirmation`) and run the `std::fs` call on
   the background executor, then refresh the local list. Guards are in the LLD.
7. Persistence: `SftpTableState { column_widths, column_visibility, expanded, local_dir }`;
   the existing 1 s debounced writer composes the document from the delegate + panel.

## Detail Design

- [x] Detail design: required (high-risk)
- Reason: local delete (recursive) and download overwrite touch the user's own disk;
  `low-level-design/local-mutations.md` fixes the guards and confirmations.
