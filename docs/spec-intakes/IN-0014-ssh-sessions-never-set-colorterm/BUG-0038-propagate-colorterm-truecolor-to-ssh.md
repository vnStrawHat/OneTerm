# Work: Propagate COLORTERM=truecolor to SSH remote shells

ID: BUG-0038
Intake: IN-0014
Created: 2026-08-24

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
- [x] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: bug
- Risk lane: normal
- Spec Intake: IN-0014

## Outcome

SSH remote shells see `COLORTERM=truecolor` (and `TERM_PROGRAM=OneTerm`) the same way local shells do, so truecolor-aware remote CLIs render correct colors.

Two layers, because OpenSSH sshd by default only accepts `AcceptEnv LANG LC_*` and silently drops other env requests:

1. The SSH connect path sends RFC 4254 §6.4 `env` requests (`COLORTERM=truecolor`, `TERM_PROGRAM=OneTerm`) after the PTY request and before the shell request — effective on servers that accept them, harmless otherwise.
2. When shell integration is enabled, the injected bootstrap prepends `export COLORTERM=truecolor;` so the variable is guaranteed inside the running remote shell.

## Scope

- [x] In scope: `crates/ssh/src/session.rs` (env requests in the PtyRequest phase + bootstrap string), owning docs note, loopback test proving the server receives the env request.
- [ ] Out of scope: per-session/user-configurable env overrides, LANG/LC_* propagation, SFTP channels.

## Acceptance

- [ ] An in-process russh server records `COLORTERM=truecolor` (and `TERM_PROGRAM=OneTerm`) via its `env_request` handler when OneTerm opens a shell channel.
- [ ] `SHELL_INTEGRATION_BOOTSTRAP` contains an unconditional `export COLORTERM=truecolor;` prefix.
- [ ] Env-request failures on servers that reject them must not abort the connect flow (want_reply = false).

## Documentation

### Owning Docs Reviewed

- `docs/terminal-backend.md` §343 — states "TERM always xterm-256color, COLORTERM=truecolor" but only for local shells; silent about SSH.
- `docs/ssh-client-connect.md` — connect/auth design; no mention of env propagation at all.
- `crates/core/src/config/env.rs` — `base_env()` is the accepted local-shell contract (TERM/COLORTERM/TERM_PROGRAM/LANG).

### Documentation Action

Update required: add a short "remote environment" note to `docs/ssh-client-connect.md` documenting the env requests and their AcceptEnv caveat plus the bootstrap fallback.

Reason: the docs currently claim COLORTERM is always set without distinguishing the SSH path, which is exactly the defect being fixed.

### Reconciliation

`docs/ssh-client-connect.md` — added §9.7 "Remote shell environment — COLORTERM" documenting both layers and the AcceptEnv caveat.

## Context

Add only relevant code, decisions, dependencies, and invariants not already clear from the owning docs.

## Plan

- [ ] Add a best-effort env request step (want_reply = false) in the `ConnectPhase::PtyRequest` closure right after `request_pty`.
- [ ] Prepend `export COLORTERM=truecolor;` to `SHELL_INTEGRATION_BOOTSTRAP`.
- [ ] Loopback test with an in-process russh server whose `env_request` handler records variables.
- [ ] Update `docs/ssh-client-connect.md`.

## Decisions

No decision record: the two-layer approach follows existing terminal-client convention (WezTerm/Kitty-style) and the accepted local-shell contract in `base_env()`.

## Verification Plan

- New unit/integration test in `crates/ssh/src/session.rs`: loopback server asserts received env variables.
- Existing bootstrap-related tests still pass (`cargo test -p oneterm-ssh`).
- Quality gate: `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo test -p oneterm-ssh --lib session::tests` — 13/13 pass, including the new loopback test `shell_channel_pushes_colorterm_and_term_program_env_requests` (in-process russh server records both env requests via its `env_request` handler) and `shell_integration_bootstrap_exports_colorterm_first`.
- `pwsh scripts/ci-local.ps1` (run twice: once to fix a `cargo fmt` diff, once clean) — all checks passed; `harness story verify BUG-0038` re-ran it: pass.
- Gap: no E2E against real OpenSSH with non-default `AcceptEnv`; layer 1 behavior on permissive servers is covered only by the in-process russh loopback test.
- Gap: layer 2 (`export COLORTERM=truecolor`) applies only when shell integration is enabled; without it, servers ignoring env requests still lack COLORTERM (documented in §9.7).

## Handoff

Use only across actors or sessions: current state, next owner/action, and blockers.
