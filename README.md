# OneTerm

A terminal app for Windows: local shells, SSH and SFTP, in tabs and spaces side by side.

OneTerm is native and GPU-rendered. It is built in Rust on
[Quadra](https://github.com/vnStrawHat/Quadra), a UI framework on Direct3D 11 and
DirectWrite, over the terminal engine [oneterm-vt](https://github.com/vnStrawHat/oneterm-vt).

**0.7 is OneTerm built again.** Versions up to 0.6.11 were another app on another toolkit,
for Windows, Linux and macOS; their source is kept in `.archived/`, as it was at the tag
`v0.6.11`, to read: nothing builds it. From 0.7.0 OneTerm is this app, for Windows x86_64 alone for now. It is made to
take over the hosts, saved passwords and settings of 0.6 at its first start, with nothing
asked, and to be the update the updater of 0.6 installs.

## What it does

- **Tabs and spaces.** Tabs in a rail at the left, folded to icons or not, or in a strip
  along the top. A tab holds spaces side by side, split either way and resized at their
  seams. Input channels send what is typed in one space to the others that joined.
- **Local shells.** The shells of the machine are found and offered in the `+` menu; shell
  integration marks prompts and tells the working directory. A release ships Microsoft's
  newer console host for throughput.
- **In a space.** Find with a count, links opened by Ctrl+click, command completion at the
  prompt, a menu for the other button, a log of the space to a file.
- **Signals.** A program's state (OSC 7501), its progress (OSC 9;4), the bell and
  notifications (OSC 9, 99, 777) show on the tab, the space's header and the status bar.
- **SSH.** Quick connect as one types it to `ssh`; saved hosts in the rail by group, with
  colour tags; host keys checked against the known hosts; passwords, key files and the
  agent; jump hosts and local forwards; keepalive; startup commands chosen by name.
- **Saved passwords.** A vault sealed under a master password (Argon2id, AES-256-GCM), which
  also locks the window when idle.
- **SFTP.** A dock beside the terminal that follows the space: columns that sort, uploads
  and downloads with their progress, properties, and an expanded view with the local folder
  beside the remote one. A remote file is edited in OneTerm's own editor or in a program of
  your choice, and saved back.
- **Settings.** Saved as they change, each with a way back to its default: themes (the
  kit's two and 37 terminal palettes), the terminal's text and cursor, keybindings, the
  network (a proxy, certificates), where the settings file is.
- **Updates.** Settings, About checks this repository's releases, fetches and verifies the
  package, installs it while the app runs, and restarts.

## Install

Download `oneterm-<version>-x86_64-pc-windows-msvc.zip` from
[Releases](https://github.com/vnStrawHat/OneTerm/releases), unpack it anywhere and run
`oneterm.exe`. There is no installer. `SHA256SUMS` of a release has the zip's digest.
Settings, Advanced says where the settings are kept.

## Build

Windows, the Rust toolchain `rust-toolchain.toml` pins, and Git Bash for the scripts.

```sh
scripts/cargo.sh run -p oneterm-app   # the app
scripts/check.sh                      # the gate: format, lints, tests (no window, no GPU)
scripts/check.sh prod                 # the same, and a release build with its tests
scripts/conpty.sh                     # Microsoft's newer console host beside the binaries (optional)
scripts/build-release.sh              # dist/oneterm-<version>-<target>.zip and its .sha256
```

`scripts/cargo.sh` is cargo with the build's temp files kept in `.tmp`. A trial run takes
`ONETERM_HOME=<folder>`, so that it reads and writes nothing of your own OneTerm. SSH and
SFTP tests connect only to servers of the test process on 127.0.0.1.

## Layout

| Path | What it is |
|---|---|
| `crates/app` | The app (package `oneterm-app`): its library and its binary `oneterm`, its themes, icon and tests |
| `crates/ssh` | SSH and SFTP over `russh` |
| `crates/completion` | Command completion: catalogs, the line at the cursor, ranking, history |
| `crates/vault` | Saved passwords: the vault's format, sealing and opening |
| `crates/shots` | The app's screens drawn to pictures without a window |
| `scripts` | The gate, the release asset, the console host, the notices, the release notes |
| `specs` | What the app does, written before the code: `oneterm.md`, `completion.md`, `decisions.md` |
| `docs/progress.md` | What is done, and what was never run for real |
| `.archived` | OneTerm 0.6's source as it was at `v0.6.11`: kept to read, built and checked by nothing |
| `.github/workflows` | `ci.yml` (the gate; licences, bans, advisories and sources by cargo-deny, `deny.toml`) and `release.yml` |

Quadra is taken by git at one commit, named in the root `Cargo.toml`; the framework is
changed in its own repository.

## Release

The app's version is `version` in `crates/app/Cargo.toml`: the version last released. The
workflow **Release** is run by hand with a bump (patch, minor, major), or by pushing a tag
`v<version>` once the manifest has that version. It builds the asset with
`scripts/build-release.sh`, commits the bump as `chore(release): v<version>` and publishes
the release with the zip and `SHA256SUMS`; its notes are made from the Conventional Commits
since the release before. The app's updater asks a release for
`oneterm-<version>-x86_64-pc-windows-msvc.zip` and checks the digest GitHub publishes for it.

## Licence

Apache-2.0 (`LICENSE`, `NOTICE`). What a release holds of others is listed in its
`THIRD-PARTY-NOTICES.txt` (`scripts/notices.py`).
