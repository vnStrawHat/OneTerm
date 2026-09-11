# US-0062 GUI walk (Windows 11, `fast-dev` build, Zed One Dark)

Driven with posted `WM_CHAR` / `WM_KEYDOWN` / mouse messages and captured with
`PrintWindow(hwnd, hdc, 2)`. The remote side was the new developer diagnostic
`cargo run -p oneterm-tools --bin sftp-dev-server -- --port 2222 --root <dir>` (loopback
SSH + SFTP, any password) serving a scratch directory with `docs/`, `logs/`, `README.txt`,
`config.toml`. OneTerm ran with `USERPROFILE`/`HOME` pointed at a scratch home (so the
host-key prompt wrote a scratch `known_hosts` and the Local pane started in the scratch
`Documents` holding `project/`, `photo.bin`, `report.txt`). The debug config directory is
`target/`: `ssh_session.json` was replaced by one `sftp-dev` session (`dev@127.0.0.1:2222`)
and `docks.json` seeded with `sftp_table_state.local_dir`; both files were restored
afterwards and the launched instance and the dev server were stopped.

| Screenshot | What it shows |
|---|---|
| `US-0062-collapsed-dark.png` | Connected, collapsed: the existing single remote pane, unchanged apart from the new toggle (panel icon) left of the "..." button. |
| `US-0062-expanded-dark.png` | After the toggle: Local pane (scratch `Documents`, Name / Date Modified / Size) left, Remote pane (`/`) right in a resizable split; tooltip "Hide the local files pane". |
| `US-0062-transfers-drag-dark.png` | After three transfers: `report.txt` uploaded with the Local toolbar arrow, `README.txt` downloaded into the local directory from the remote context menu (no save dialog; the local list refreshed), `photo.bin` uploaded by dragging its local row onto the remote list. The queue lists all three as Done. A fourth transfer, `config.toml`, was downloaded by dragging a remote row onto the local list (server log: `open "/config.toml"`, `read`). |
| `US-0062-local-menu-dark.png` | Local row context menu: Upload, Rename, Delete, New Folder, Refresh. The Local "..." menu shows New Folder, Upload, Rename, Delete, Refresh. |
| `US-0062-replace-prompt-dark.png` | Download of `README.txt` while a local `README.txt` exists: "Replace Local File" confirmation; Escape cancelled it and no transfer started. |
| `US-0062-local-delete-confirm-dark.png` | New Local Folder created `walk-folder` (toast "Folder "walk-folder" created."); Delete on it shows the recursive-folder wording; confirming removed the folder (toast "Deleted successfully."). Submitting the New Folder dialog empty showed "Name cannot be empty." and kept the dialog open. |
| `US-0062-collapsed-again-dark.png` | Toggle again: back to the single remote pane; `docks.json` recorded `expanded: false`. Earlier in the walk a restart of the app with `expanded: true` persisted from the previous run opened straight into the dual-pane layout (log: "SftpPanel: local pane shown" at startup). |

Server-side check (dev server request log): the upload wrote `/.report.txt.oneterm-<pid>-1.part`
then `rename` → `/report.txt`; the drag upload did the same for `photo.bin` (two `write`
packets); the downloads issued `lstat` + `open` + `read` on `/README.txt` and
`/config.toml`. Files on disk matched the screenshots (`remote-root` gained `report.txt`
and `photo.bin`; the scratch `Documents` gained `README.txt` and `config.toml`).

Not walked: Rename in the Local pane (same dialog and `validate_entry_name` as the remote
rename; the existing-target refusal is covered by the LLD design and unit-level validation
of the name rules), the local path box with a typed drive path, and sorting by column in the
Local pane (unit-tested `sort_local_entries`).

## Acceptance rework (2026-09-11): expand behaves like the terminal tab zoom

Owner feedback after the first walk: expand/collapse must work like expanding/collapsing a
terminal tab. The hosting `SshClientPanel` now zooms its dock node when the browser expands
(and hides the Session section), and any zoom-out collapses the browser. Same driver and
loopback server as above.

| Screenshot | What it shows |
|---|---|
| `US-0062-rework-expanded-zoomed-dark.png` | Connected, after the toggle (now the terminal tab's Maximize icon): the SFTP Browser fills the whole workspace — no center tabs, no Session section — with Local (scratch `Documents`) left and Remote (`/`) right; the Minimize icon sits in the remote toolbar. Log: `SshClientPanel: sftp expanded=true, node=Some(NodeId(10)), zoomed=None` then `dock zoom = true`. |
| `US-0062-rework-collapsed-docked-dark.png` | After the Minimize toggle: the terminal tab and the Session + SFTP split are back in the right dock, single remote pane, Maximize icon in the toolbar. Log: `dock zoom = false`. |
| `US-0062-rework-startup-no-connection-dark.png` | Start with `sftp_table_state.expanded = true` and no SSH session: the persisted state re-expands, the node zooms, and the no-connection state still shows the Local pane, a "Remote" bar with the Minimize toggle, and "No SFTP connection." — the user is never trapped in the zoom. A normal window close then wrote `docks.json` `zoomed_panel = "ssh_client_panel"`. |
| `US-0062-rework-startup-zoom-restored-dark.png` | Start with `zoomed_panel = "ssh_client_panel"` and `local_dir` = the scratch `Documents`: the shell restores the zoom, `SshClientPanel::set_zoomed(true)` expands the browser (log: `Restored zoom for panel "ssh_client_panel"` → `dock zoom = true` → `local pane shown`), the pane first lists the home directory and then reloads the persisted `Documents` once `docks.json` has been read (two `LocalPane::load_dir` lines). |
| `US-0062-rework-collapsed-no-connection-dark.png` | The Minimize toggle in that no-connection state docks the panel again (terminal + Session + SFTP); the close afterwards wrote `zoomed_panel = None`, `expanded = false`. |

Found and fixed during this walk: the first host-panel version zoomed the dock from inside
its own event handler and panicked (`cannot read SshClientPanel while it is already being
updated` — the tab group asks the panel whether it is zoomable); the zoom is now deferred with
`Window::defer`. A zoom restored at startup expanded the Local pane before the persisted
`local_dir` arrived, so the pane opened in the home directory; `set_initial_dir` now reloads
an already-open pane with the persisted directory.

Observed shell behaviour, not changed here: after `restore_zoom` at startup, closing the window
without any other layout change writes `zoomed_panel = None` (the workspace's dock observer
does not record the restored zoom). The SFTP state `expanded = true` is persisted
independently and re-expands (and re-zooms) on the next start, so the browser still comes back
expanded.

## Follow-up (2026-09-11): toggle moved to the panel title

Owner request: put the Expand/Collapse button on the panel title via `title_suffix`. The
button is now `SftpPanel::title_suffix`, drawn by `SshClientPanel` at the trailing end of the
"SFTP Browser" header (docked and zoomed alike); the toolbar and the no-connection layout
no longer carry it. Walked without a connection:

| Screenshot | What it shows |
|---|---|
| `US-0062-title-toggle-docked-dark.png` | Docked: the Maximize icon sits at the right end of the "SFTP Browser" header, inside the same framed control group as the terminal tab bar's trailing buttons — full header height, a left border, the tab-bar padding (owner follow-up: "like the terminal"; the capture shows both bars side by side); the toolbar ends with "..." again. |
| `US-0062-title-toggle-zoomed-dark.png` | After clicking it: zoomed Local + Remote layout, Minimize icon (in the same framed group) at the right end of the full-width "SFTP Browser" header (log: `local pane shown` → `dock zoom = true`); clicking it again docked the panel (`dock zoom = false`). |
