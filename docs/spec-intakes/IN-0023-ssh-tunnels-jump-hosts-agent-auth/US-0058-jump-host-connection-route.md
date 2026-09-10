# Work: Jump host connection route

ID: US-0058
Intake: IN-0023
Created: 2026-09-10

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability
- Risk lane: high_risk (authentication on every hop; host-key trust per hop; secrets for several hosts in RAM)
- Spec Intake, when required: IN-0023

## Outcome

A saved session can name another saved session as its jump host (chains up to four hops).
Connect opens each hop in order, authenticates it with its own method, opens a direct-tcpip
channel to the next hop, and runs the target handshake over that channel. Host-key prompts
and errors name the hop. Credentials for every hop are entered in one connect dialog and never
persisted.

## Scope

- [ ] In scope: `SshHop`, `SshConfig.jump_hops`, `MAX_JUMP_HOPS`, `route()` (`oneterm-core`);
  `crates/ssh/src/route.rs` (`open_transport`, `JumpHandles`, `hop_error`) and the
  `ssh_main_task` handle ownership; `SshSession.jump_host`, `resolve_jump_chain`, jump-host
  combobox in the session dialog and Quick Connect, per-hop credential blocks in the connect
  dialog, hop-aware host-key confirmation, Duplicate Session prompting per hop; tests.
- [ ] Out of scope: `ProxyCommand`; client-side HTTP/SOCKS proxies; per-hop keepalive
  settings; counting jump-hop bytes in the bandwidth indicator.

## Acceptance

- [ ] Session dialog offers "Jump host: None | <other saved sessions>"; the edited session is not listed; saving a cycle or a chain longer than four hops is rejected with a message.
- [ ] Connect to a session with one jump host shows two credential blocks (hop then target) and opens a shell on the target; `who` on the target shows the bastion as the origin.
- [ ] Two-hop chain works the same way with three blocks.
- [ ] Unknown host key on the jump host: the confirmation dialog names the jump host and "(jump host for <label>)"; accepting once connects; the target's own unknown key prompts separately.
- [ ] Bastion with `AllowTcpForwarding no`: the error reads `jump host user@host:port: ...` and names the next hop's address.
- [ ] Deleted jump host: connect reports the missing session; the dialog shows "None (missing session)".
- [ ] Closing the tab closes the target and every hop (no lingering TCP connections in `netstat`).
- [ ] `ssh_session.json` without `jump_host` loads unchanged; no secret is written.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/high-level-design.md` — data flow steps 2, 3, 8; wireframes.
- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/low-level-design/jump-hosts.md` — route walk, handle lifetime, error attribution, chain resolution.
- `docs/decisions/DEC-0010-jump-hosts-reference-saved-sessions.md` — reference model, per-hop credentials, chain cap.
- `docs/ssh-client-connect.md` — connect phases, host-key flow, `SshConfig` never serialized.
- `docs/ssh-authentication.md` — per-hop application of the secret policy.
- `docs/decisions/0002-ssh-duplicate-auth.md` — duplicate prompts again, now per hop.
- `docs/agents/persistence.md` — `ssh_session.json` additive field.
- `docs/terminal-backend.md` §SSH — `ssh_main_task` ownership and teardown order.

### Documentation Action

- Update required: `docs/ssh-client-connect.md` (route section: hops, `connect_stream`, per-hop phases and errors, teardown order); `docs/ssh-authentication.md` (per-hop credentials); `docs/agents/persistence.md` (`jump_host`); `docs/agents/structure.md` (`crates/ssh/src/route.rs`).

Reason: the connect flow document describes one TCP connection and one authentication; the route changes both.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `russh::client::connect_stream(config, stream, handler)` accepts any `AsyncRead + AsyncWrite + Unpin + Send + 'static`; `Channel::into_stream()` provides one; `Handle::channel_open_direct_tcpip(host, port, originator_ip, originator_port)`.
- `Handle` is not `Clone`; today it is moved into `ssh_main_task` as `_handle` and dropped in the teardown block after `disconnect` (`crates/ssh/src/task.rs`).
- `SshClientHandler::new(host, port, policy)` and the `UnknownHostKey` / `HostKeyChanged` errors already carry host and port (`crates/core/src/error.rs`), which the UI uses to pick the hop.
- `open_host_key_confirmation` (`crates/session-ui/src/common.rs`) mutates `cfg.host_key_policy` on the retried config.
- `group_combo.rs` is the combobox pattern to reuse for the jump-host picker.

## Plan

- [ ] Core types + `route()` + tests.
- [ ] `route.rs`: `open_transport`, `JumpHandles` (reverse-order drop), `hop_error`; `connect` loops over the route; `ssh_main_task` takes `_jump_handles`.
- [ ] Two-server test (`connect` through hop; refused direct-tcpip; unknown hop key).
- [ ] `SshSession.jump_host`, `resolve_jump_chain` + tests (missing, cycle, too long).
- [ ] Dialog combobox (session + Quick Connect), connect dialog blocks, host-key text, Duplicate Session.
- [ ] Docs + gate.

## Decisions

- DEC-0010 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual: real bastion + target; unknown-key prompts per hop; `netstat` after close.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

After implementation, record commands, results, and anything skipped, unavailable, partial, or failing.

## Handoff

Not started. Shares the `authenticate` helper with US-0057; whichever lands first extracts it.
