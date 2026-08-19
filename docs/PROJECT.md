# Project Context

Short, factual project memory for the Harness flow (`docs/HARNESS.md`). Only facts taken from
the code, manifests, tests, and accepted decisions belong here — not speculative plans.

## Mode

`brownfield`

Existing code is authoritative where a contract has not yet been documented. Missing
documentation means "inspect and preserve current behavior", not "design freely".

## Purpose

OneTerm is a desktop terminal application for SSH, SFTP, and local shells with a Zed-style
workspace UI (dock area, tabs, split Spaces). It also monitors coding agents through the
OSC 9;7 proposal ([`docs/osc-agent-status.md`](osc-agent-status.md)). Users are developers who
run interactive shells and transfer files against remote hosts; the repository owns the whole
product: terminal engine glue, backends, UI, persistence, packaging, and the auto-updater.

## Stack and Surfaces

- Rust (edition 2024, toolchain pinned in `rust-toolchain.toml`), one Cargo workspace under
  `crates/` (`oneterm-<dir>` packages; graph in `docs/agents/structure.md`).
- UI: `gpui` + `gpui-component` (pinned git revisions, `docs/agents/dependencies.md`);
  `gpui-component`, `alacritty_terminal`, and `vte` are vendored forks under `vendor/`
  (pristine upstream + `vendor/patches/`).
- Terminal engine: `alacritty_terminal`; local PTY via `alacritty_terminal::tty` (Windows ConPTY
  with bundled `conpty.dll` / `OpenConsole.exe`); SSH/SFTP via `russh` + `russh-sftp` on a
  tokio runtime hidden inside `crates/ssh`.
- Binary: `oneterm` from `crates/app` (keeps the console in debug builds); local
  diagnostics binaries live in `crates/tools`.
- Persistence: JSON files in `oneterm_core::config_dir()` (`target/` in debug builds,
  `~/.OneTerm/` in release): `terminal.json`, `ui_config.json`, `docks.json`,
  `ssh_session.json`, `update_config.json`, plus `crashes/` and the SFTP
  `edit-cache/<pid>/` (transient local copies of remote files opened with the
  SFTP "Edit" action; per-process so concurrent instances stay isolated. Each
  `<pid>` dir is pruned once empty; a startup sweep reclaims `<pid>` dirs whose
  process is no longer alive — covering runs that were killed before cleanup ran)
  (schemas: `docs/agents/persistence.md`).
- Distribution: GitHub Releases built by `.github/workflows/release.yml`; the in-app updater
  (`crates/update`) downloads from the same releases.
- Platforms: Windows is the primary, tested platform; Linux and macOS compile and are packaged
  but are not yet QA-tested (`README.md`).

## Important Boundaries

- Terminal-controlled input (PTY / SSH output, OSC payloads, paste): sanitised through
  `crates/terminal` policies before it reaches the UI or the clipboard.
- SSH servers: host keys are checked against `known_hosts` with a confirmation dialog;
  authentication material (passwords, key passphrases) stays in memory only
  (`docs/decisions/0001-ssh-key-secret-persistence.md`).
- SFTP: remote paths are `RemotePath` values; local destinations are contained under the chosen
  directory; transfers run behind `SftpBackend` (`crates/core/src/sftp.rs`).
- Persisted files: read/updated through `crates/core/src/persistence.rs` (atomic replace,
  backup, quarantine); `docks.json` is owned by `crates/state/src/dock_persistence.rs`.
- Auto-update: only `https://` GitHub Releases of the canonical repository
  (`crates/update/src/config.rs`; `ONETERM_UPDATE_REPO` overrides at build time), SHA-256
  verified archives, staged install with rollback.
- Public contracts: `TerminalSession`, `SessionFactory` (`crates/terminal`), `SftpBackend`
  (`crates/core`), `AppServices` / `WorkspaceCommands` (`crates/state`), registered dock panel
  names (`crates/state/src/panel_names.rs`, persisted in `docks.json`).

## Invariants

- Crate rules R1–R12 in `docs/agents/crate-dependency-rules.md` (DAG, downward-only edges,
  no UI→backend edge, feature-agnostic shell, `core`/`terminal` gpui-free).
- Every third-party dependency is declared once in the root `Cargo.toml`
  `[workspace.dependencies]`; `gpui`/`gpui_platform` and `gpui-component`/`-assets` share revs.
- Vendored crates are never hand-edited: `vendor/<crate>` == pristine @ rev + `vendor/patches/`
  (`bash vendor/refresh.sh --check`, `python scripts/check-ui-fork.py`).
- No secrets are persisted (`ui_config.json`, `terminal.json`, `docks.json`, `ssh_session.json`
  never contain passwords or passphrases).
- English-only contributor text and code comments (`python scripts/check-english.py`).
- Every crate inherits `[workspace.package] version` (`python scripts/verify-dependency-graph.py`).
- Do not hard-code colours in components — read `cx.theme()` / `TerminalTheme`.

## Verification

```text
Focused:      cargo test -p <crate>            (unit tests live next to the code)
Unit:         cargo test --workspace
Integration:  cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh
              (portable backend contracts; CI runs them on ubuntu/macos/windows)
Quality gate: scripts/ci-local.sh  |  pwsh scripts/ci-local.ps1   (mirrors .github/workflows/ci.yml)
Release:      pwsh scripts/build-release.ps1  |  scripts/build-release.sh  (local packaging)
              .github/workflows/release.yml   (tag push v* or workflow_dispatch)
```

There is no automated end-to-end UI test; UI behaviour is verified manually on Windows.

## Open Questions

- Linux/macOS QA: local PTY, packaging, and theming are untested outside Windows.
- Whether the gpui-component fork can be retired once its two source patches are upstreamed
  (`docs/agents/ui-fork-maintenance.md`).
