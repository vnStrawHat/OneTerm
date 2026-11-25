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

## Not run yet

Built and tested without the real thing; each wants a look before it is relied on:

- Taking over from a real folder of OneTerm 0.6 (tests use a folder made by the test).
- A request of the updater to GitHub: the tests answer from 127.0.0.1. So also the hop from
  0.6.11 to 0.7.0 by the updater of OneTerm 0.6, and Restart into a newer version from a
  real release.
- Certificates unchecked (the network's setting): built, not exercised against a server
  with a bad certificate. Proxies: HTTP ones named `host:port` only; no SOCKS, no
  credentials in the address.
- The workflows: neither `ci.yml` nor `release.yml` has run on GitHub. What they call was
  run by hand on an export of the repository: the gate, cargo-deny 0.22.1 (all four
  checks), and `scripts/build-release.sh`.
- A real window's drag across monitors, DPI change and snap are checks by hand.

## Known limits

- Windows x86_64 alone.
- The git counts of the status bar are of tracked files against the last commit.
- Renaming a startup command is saving the new name and removing the old one: hosts that
  chose the old one go back to None.
- In `settings.json` the vault is one string of JSON, and the saved hosts are rows of words.
