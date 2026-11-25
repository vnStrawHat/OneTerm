# OneTerm: working notes for Claude

OneTerm is a terminal app for Windows: local shells, SSH and SFTP, in tabs and spaces side by
side. It is built in Rust on Quadra (github.com/vnStrawHat/Quadra), the user's native
GPU-rendered UI framework, over the user's terminal engine `oneterm-vt`. Versions before 0.7
("OneTerm 0.6") were another app on another toolkit. Its source is kept in `.archived/`, as
it was at the tag `v0.6.11`: the reference for behaviour and settings the design leaves
open. It is read, never edited, built or brought back; the gate leaves it out, and its own
`CLAUDE.md` and `AGENTS.md` were for that app, not for this one.

## Hard rules from the user (never break them)

1. **Stay inside the project directory**: scratch files in `.tmp/`, downloads and caches in
   `.cache/`. Never use the system temp dir, `mktemp` or the home dir. **One exception**: the
   Rust toolchain and cargo's build output live outside (the cargo home).
2. **No PowerShell**: Git Bash + coreutils only, never `.ps1` (the gate rejects them).
3. **Conventional Commits** for every commit message (`feat(app): ...`, `fix(ssh): ...`,
   `docs(specs): ...`, `refactor: ...`, `test: ...`, `chore: ...`, `perf: ...`; `!` or a
   `BREAKING CHANGE:` footer for a breaking change). Scopes are the crates' folders (app, ssh,
   completion, vault, shots), or specs, scripts, ci. The release notes are made from them.
4. **Never test the real clipboard.**
5. **Commit each finished part** once `scripts/check.sh prod` exits 0, then go on with the
   next; **push only when the user asks**.
6. **Releasing is the user's act.** Never publish a release, never make or push a tag, never
   run the release workflow. The manifest's version (`crates/app/Cargo.toml`) is the version
   last released; the user bumps it through the release workflow. Never write the next
   version there.

## How to work

- Chat with the user in **Vietnamese**; code, comments, docs, specs and commits in
  **English**.
- **Specs come before code**: change `specs/` first, then implement, then update
  `docs/progress.md` (the source of truth for what is done). The user's decisions are in
  `specs/decisions.md`.
- **Performance first**, and work proportional to the change: a terminal that drops frames or
  rebuilds its UI is not done.
- **The framework is Quadra's.** Windows, layout, text, rendering, the kit's components, the
  terminal view, the updater: a change to any of them is made in Quadra's repository, then
  this repository moves to the commit that has it (`rev` in the root `Cargo.toml`, the one
  place) and the lock file follows. Nothing of Quadra is copied or patched here.
- **Few dependencies, not zero.** A new crate needs a row in `specs/decisions.md` and must
  pass `deny.toml` (the CI runs cargo-deny).
- **No `unsafe`**: the workspace's lints forbid it in every crate. What needs it is Quadra's.
- Files stay **at most 300 lines** (the gate).
- Shipped code starts no program that Windows may not have (`tar.exe`, `curl.exe`): an API
  of the OS or an admitted crate does the work.
- **A view follows Quadra's design language** (Quadra's `specs/design`: read its `layout.md`,
  `patterns.md` and `checklist.md` before building or changing a view). A view is built from
  `vstack`, `hstack`, `form`, `columns`, `button_row`, `section` and the containers (dialog,
  popover, card), which carry the distances: it writes no gap, padding, radius or text size
  of its own, and a field in a form takes no width. A number that must be written is a step
  of a ladder, by its token; `scripts/check_design.py` (in the gate) rejects the others.
- Tests drive controls through the semantic tree (`get_by_role` and the like) on the headless
  platform, with no window and no GPU. SSH and SFTP tests run only against servers of the
  test process on 127.0.0.1; the updater's tests too. No test asks a real server anything.
- A trial run takes `ONETERM_HOME=.tmp/oneterm` so that it reads and writes nothing of the
  user's own OneTerm, and is closed by its own process alone: the user may be running theirs.
- The limited-RAM machine: do not run two release builds at once.
- Edits by script: Python 3 with `PYTHONUTF8=1`, opening files with `encoding="utf-8",
  newline=""`.
- The remote is `origin`, github.com/vnStrawHat/OneTerm. Push only when the user asks.
- Habits that cost time when forgotten:
  - Read the gate's exit status, not its output through a pipe:
    `scripts/check.sh prod > .tmp/check.log 2>&1; echo $?`, and commit only on 0.
  - A script with backslashes is written to `.tmp/` as a file, not passed through a
    here-document (the shell collapses the backslashes).

## Commands (Git Bash, from the repository's root)

| What | Command |
|---|---|
| Any cargo command | `scripts/cargo.sh <args>` (temp files stay in `.tmp`) |
| Run the app | `scripts/cargo.sh run -p oneterm-app` (a trial: `ONETERM_HOME=.tmp/oneterm`) |
| Format | `scripts/cargo.sh fmt --all` |
| Gate | `scripts/check.sh` |
| Release gate | `scripts/check.sh prod`; run both before saying done |
| One crate's tests | `scripts/cargo.sh test -p oneterm-ssh` |
| The dependency policy | `cargo deny check licenses bans advisories sources` (`deny.toml`; the CI runs it) |
| The console host | `scripts/conpty.sh` puts Microsoft's newer `conpty.dll` and `OpenConsole.exe` beside the binaries; OneTerm uses the system's without them |
| The release asset | `scripts/build-release.sh` makes `dist/oneterm-<version>-<target>.zip` and its `.sha256`; it publishes nothing |
| The app's screens as pictures | `scripts/cargo.sh run -p oneterm-shots -- .tmp/shots [scene ..]` draws them without a window |
| Numbers off the design's ladders | `python scripts/check_design.py --list` |

Target: `x86_64-pc-windows-msvc`. Toolchain pinned in `rust-toolchain.toml`.

## Map

- `specs/README.md` indexes the specs: `specs/oneterm.md` (the app), `specs/completion.md`
  (the completion engine), `specs/decisions.md` (the user's decisions, the dependencies).
- `docs/progress.md`: what is done, what was never run for real, the known limits.
- `.archived/`: OneTerm 0.6's source, to read only (above).
- `.github/workflows`: `ci.yml` (the gate on Windows, cargo-deny) and `release.yml` (run by
  the user: by hand with a bump, or by a tag `v*`).

| Crate | What it is |
|---|---|
| `crates/app` (`oneterm-app`) | The app: the library `oneterm` (the window, the rail, spaces, settings, the lock, the dock; one `Model` the views share) and the binary `oneterm`; themes, the icon, the tests |
| `crates/ssh` (`oneterm-ssh`) | SSH and SFTP over `russh` on a `tokio` runtime of its own, behind a blocking API; its `testing` feature is the test server |
| `crates/completion` (`oneterm-completion`) | Command completion: catalogs, the line at the cursor, ranking, history, redaction; no view |
| `crates/vault` (`oneterm-vault`) | Saved passwords: the vault's document and envelopes (OneTerm 0.6's format), sealing and opening; it knows nothing of the app |
| `crates/shots` (`oneterm-shots`) | The app's screens drawn to pictures, to compare with the design |

