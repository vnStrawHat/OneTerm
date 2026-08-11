# Project Context

## Mode

`brownfield`

Existing code is authoritative where a contract has not yet been documented.

## Purpose

OneTerm is a Rust desktop terminal application for users who need SSH, SFTP, and local shell sessions in a Zed-style workspace UI. It also monitors coding agents through OSC 9;7 status events.

## Stack and Surfaces

- Language: Rust workspace.
- Desktop UI: GPUI and the pinned, locally patched gpui-component dependency.
- Terminal engine: alacritty_terminal.
- Protocol/runtime surfaces: SSH, SFTP, local PTY, app updater, settings, workspace docking, and agent status UI.
- Local storage: configuration and state files under OneTerm's platform configuration directory.
- External providers: GitHub Releases for updates and GitHub Issues for user-reviewed crash issue drafts.
- Primary release target: desktop application, including a Windows release binary and cross-platform GPUI code paths.

## Important Boundaries

- `oneterm-app` is the only crate that wires shell, features, and protocol backends together.
- UI crates do not depend directly on SSH or local-shell backends.
- User-owned JSON persistence uses the shared core persistence mechanics and domain-owned schemas.
- GPUI/gpui-component APIs are researched from `reference/gpui-component` before external sources.
- Crash reports are local diagnostic text and are not uploaded automatically.

## Invariants

- Crate dependencies follow `docs/agents/crate-dependency-rules.md` R1-R12.
- Recoverable runtime errors do not become process panics.
- Blocking persistence work does not run directly in UI action handlers.
- Components use theme colors rather than hardcoded UI colors, except existing branded assets.
- Rust code and repository documentation content are written in English.

## Verification

```text
Focused: cargo test -p <affected-package>
Unit: cargo test --workspace
Integration: cargo test --workspace
End-to-end: manual desktop UI flows where no automated GPUI harness exists
Release: cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo build --workspace
```

## Open Questions

- Whether future releases should add platform-native fatal signal/exception capture beyond Rust panics.
