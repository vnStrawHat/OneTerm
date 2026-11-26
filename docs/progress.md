# Progress

What is done, by the phases of `specs/oneterm.md`; the source of truth for the app's state.
OneTerm was built inside Quadra's repository from 2026-11-15 to 2026-11-25; each step's
story, with its tests and pictures, is in that repository's `docs/progress.md` and history.

## Phases

| Phase | Status |
|---|---|
| **P1 Local** (M1 to M6) | Done: the window and its bars, tabs in a rail (folded or not) or in a strip, spaces side by side, the signals, the `+` menu, the status bar, settings and themes, the master password and the lock |
| **P2 Remote** (M7 to M10) | Done: the SSH engine and quick connect, saved hosts, the SFTP dock and its expanded view, jump hosts and local forwards. Tests run against servers of the test process on 127.0.0.1 only |

| Id | Contents | Status |
|---|---|---|
| M1 | The window and its title bar, one local shell, the status bar | Done |
| M2 | Tabs and spaces, their keys, the horizontal variant | Done |
| M3 | Signals: OSC 7501 status, OSC 9;4 progress, the bell, notifications (OSC 9, 99, 777) | Done |
| M4 | The `+` menu, the shells found, shell integration, the status bar's git, CPU, memory | Done |
| M5 | Settings (saved as they change), themes, keybindings | Done |
| M6 | The master password and the lock screen | Done |
| M7 | SSH: connect, host keys, authentication, the Connect dialogs | Done |
| M8 | Saved hosts in the rail, groups and tags, saved passwords (the vault), key files | Done |
| M9 | The SFTP dock, transfers, the expanded view, editing a remote file | Done |
| M10 | Jump hosts and local forwards | Done |

## Since the phases

- Find in a space, input channels, links, a space's menu, logging a space to a file.
- Command completion at the prompt (`specs/completion.md`).
- Taking over from OneTerm 0.6: hosts, the vault and its sealed passwords, settings, the
  proxy; automatic, once, silent. The folders of the name before (1Term) are adopted.
- Updates in Settings, About: check, fetch and verify, install while the app runs, restart.
- The network's settings (a proxy of the system or named by hand, certificates checked or
  not), which the updater follows.
- Startup commands as a list of the settings, chosen by name in a host's dialog.
- The SFTP dock's header of columns that sort (Name, Size, Modified, Permissions).
- The status bar's git says the lines added and removed.
- 2026-11-25: the app is a set of crates (`app`, `ssh`, `completion`, `vault`, `shots`) with
  its own scripts (the gate, the release asset), workflows (CI with the dependency policy,
  release) and specs, laid out as this repository.
- 2026-11-25: the status bar is 28 px, lower than the title bar (32): the user's decision.
- 2026-11-25: the status bar says the focused space's columns and rows before CPU and
  memory, following a resize (Quadra's `Session::grid`); a setting turns it off.
- 2026-11-25: new spaces start in the home folder unless the setting says otherwise. The
  SFTP dock has lines under its header and rows and, as each pane of the expanded view,
  Go, Refresh and a menu of options (Sync CWD, Show hidden files) after its path. A user
  not given is asked for in an empty field.
- 2026-11-25: the SFTP paths are underlined fields (the dock's and both panes'). In a
  space two clicks select a word, three the line, and Shift and a click reaches from the
  selection there is (Quadra's fix; `tests/select.rs`).
- 2026-11-26: a saved host's row in the `+` menu (under Recent too) and in the palette opens
  its Connect dialog, as its row in the rail does; the `+` menu's hosts of no group are under
  `SSH hosts`, not under Recent; the host editor has Shell integration beside Auth method,
  and a key beside its passphrase. The Connect dialog's fields are as wide as the dialog
  whatever the auth (they were as wide as their words once the host's key was known), and
  `Delete host…` is in the danger colour.
- 2026-11-25: SFTP: more than one row chosen (Ctrl and Shift with a click) to download,
  upload or delete together; Open in terminal types `cd` to a folder into the shell; a
  mode is set from Properties (`Files::chmod`); Sync CWD and Show hidden files are kept.
- 2026-11-25: 0.7.0 is released, by the release workflow (the user's word).
- 2026-11-25: a connection lost is asked about, and Reconnect connects again in the same
  space. A saved host has its own switch of shell integration. A command palette
  (`Ctrl+Shift+P`): the app's commands, the tabs, the saved hosts, the theme.
- 2026-11-25: a tab shows the input channels its spaces are in, in the rail, the folded
  rail and the strip (a tab of one space showed none anywhere).
- 2026-11-25: shell integration for a host's shell (a line typed unseen once it is open), so
  a remote space says its directory: the status bar, the space's header, Sync CWD. Git shows for a local space alone.

## Not run yet

Built and tested without the real thing; each wants a look before it is relied on:

- Taking over from a real folder of OneTerm 0.6 (tests use a folder made by the test).
- A request of the updater to GitHub: the tests answer from 127.0.0.1. So also the hop from
  0.6.11 to 0.7.0 by the updater of OneTerm 0.6, and Restart into a newer version from a
  real release.
- Certificates unchecked (the network's setting): built, not exercised against a server
  with a bad certificate. Proxies: HTTP ones named `host:port` only; no SOCKS, no
  credentials in the address.
- Shell integration over SSH: the user saw Sync CWD follow the shell of a real server
  (2026-11-25). Open in terminal has not been tried on one, nor a zsh or a dash as the
  server's shell.
- A real window's drag across monitors, DPI change and snap are checks by hand.

## Known limits

- Windows x86_64 alone.
- The git counts of the status bar are of tracked files against the last commit.
- Renaming a startup command is saving the new name and removing the old one: hosts that
  chose the old one go back to None.
- In `settings.json` the vault is one string of JSON, and the saved hosts are rows of words.
