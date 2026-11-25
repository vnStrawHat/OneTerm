# Work: Jump host connection route

ID: US-0058
Intake: IN-0023
Created: 2026-09-10

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [x] In progress
- [x] Implemented
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

- [x] In scope: `SshHop`, `SshConfig.jump_hops`, `MAX_JUMP_HOPS` (`oneterm-core`; `route()` was not needed, `connect` iterates the hops then the target directly);
  `crates/ssh/src/route.rs` (`open_transport`, `JumpHandles`, `hop_error`) and the
  `ssh_main_task` handle ownership; `SshSession.jump_host`, `resolve_jump_chain`, jump-host
  combobox in the session dialog and Quick Connect, per-hop credential blocks in the connect
  dialog, hop-aware host-key confirmation, Duplicate Session prompting per hop; tests.
- [x] Out of scope: `ProxyCommand`; client-side HTTP/SOCKS proxies; per-hop keepalive
  settings; counting jump-hop bytes in the bandwidth indicator.

## Acceptance

- [x] Session dialog offers "Jump host: None | <other saved sessions>"; the edited session is not listed; saving a cycle or a chain longer than four hops is rejected with a message (picker walked in the GUI; cycle / length rejection unit-tested in `jump_chain`, the Save path calls it).
- [ ] Connect to a session with one jump host shows two credential blocks (hop then target) and opens a shell on the target; `who` on the target shows the bastion as the origin (dialog walked in the GUI; the shell through a bastion is proven by the in-process two-server test, not against a real host).
- [ ] Two-hop chain works the same way with three blocks (chain resolution unit-tested; not walked).
- [ ] Unknown host key on the jump host: the confirmation dialog names the jump host and "(jump host for <label>)"; accepting once connects; the target's own unknown key prompts separately (backend attribution unit-tested: an unknown key behind the bastion names the target; the dialog text needs a real host).
- [x] Bastion with `AllowTcpForwarding no`: the error reads `jump host user@host:port: ...` and names the next hop's address (unit-tested against a refusing in-process bastion + `hop_error`).
- [x] Deleted jump host: connect reports the missing session (`JumpChainError::Missing`, shown as a notification before the dialog opens); the edit dialog shows "None" for a dangling reference and Save clears it (the picker ignores an unknown id).
- [ ] Closing the tab closes the target and every hop (`JumpHandles` dropped after the target in the teardown block; `netstat` check needs a real host).
- [x] `ssh_session.json` without `jump_host` loads unchanged; no secret is written.
- [x] `pwsh scripts/ci-local.ps1` green.

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

- `docs/ssh-client-connect.md` — §9.2 names the per-hop phases and the hop-prefixed error row; §9.6 now lists the agent method (it still called agent auth a roadmap item after US-0057; fixed here); new §9.8 "Connection route — jump hosts" (transport, per-hop host keys, credential zeroization, `JumpHandles` drop order, saved reference model).
- `docs/ssh-authentication.md` — secret policy applied per hop.
- `docs/agents/persistence.md` — `jump_host` field and its dangling-reference rule.
- `docs/agents/structure.md` — `crates/ssh/src/route.rs`, `crates/session-ui/src/jump_hops.rs`.
- `docs/terminal-backend.md` — teardown block drops the target then the hop handles.
- `docs/decisions/0002-ssh-duplicate-auth.md` — reviewed, unchanged: duplicates now carry `SshDuplicateHop` metadata and prompt per hop, which is the same rule applied to more hosts.

## Context

- `russh::client::connect_stream(config, stream, handler)` accepts any `AsyncRead + AsyncWrite + Unpin + Send + 'static`; `Channel::into_stream()` provides one; `Handle::channel_open_direct_tcpip(host, port, originator_ip, originator_port)`.
- `Handle` is not `Clone`; today it is moved into `ssh_main_task` as `_handle` and dropped in the teardown block after `disconnect` (`crates/ssh/src/task.rs`).
- `SshClientHandler::new(host, port, policy)` and the `UnknownHostKey` / `HostKeyChanged` errors already carry host and port (`crates/core/src/error.rs`), which the UI uses to pick the hop.
- `open_host_key_confirmation` (`crates/session-ui/src/common.rs`) mutates `cfg.host_key_policy` on the retried config.
- `group_combo.rs` is the combobox pattern to reuse for the jump-host picker.

## Plan

- [x] Core types (`SshHop`, `MAX_JUMP_HOPS`, `SshDuplicateHop`) + tests.
- [x] `route.rs`: `open_transport`, `authenticate` (the extraction US-0057 deferred), `connect_hop`, `JumpHandles` (reverse-order drop), `hop_error`; `connect` loops over the hops then the target; `ssh_main_task` drops the target before the hops.
- [x] Two-server tests (session channel through a relaying bastion; refused direct-tcpip; unknown key behind the bastion names the target; `hop_error` shape).
- [x] `SshSession.jump_host`, `SshSessionStore::jump_chain` + tests (outermost-first order, missing, cycle, self-loop, too long).
- [x] `jump_hops.rs`: `HopSpec`, `JumpHopForms`, `JumpHostPicker` (a `Select` over the other saved sessions); session dialog + Quick Connect pickers, hop credential blocks in the connect and Quick Connect dialogs, hop-aware host-key confirmation, Duplicate Session prefill.
- [x] Docs + gate + GUI walk (`evidence/US-0058-gui-walk.md`).

## Decisions

- DEC-0010 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual: real bastion + target; unknown-key prompts per hop; `netstat` after close.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/ssh-jump-hosts`, 2026-09-10):

- `rtk proxy cargo test -p oneterm-ssh -p oneterm-core` — 58 + 47 passed; the four new route
  tests run two in-process `russh` servers (a relaying bastion and a session-only target).
- `rtk proxy cargo test -p oneterm-session-ui` — 48 passed (chain resolution, `jump_host`
  round-trip, `HopSpec` from a saved session and from duplicate metadata, the form rejecting a
  stale key path for agent auth).
- `rtk proxy cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`, 1091 tests.
- GUI walk: `evidence/US-0058-gui-walk.md` with five screenshots (Quick Connect rows and
  dropdown, the hop block after picking a jump host, the connect dialog with the bastion block
  focused above the target, the edit dialog with the saved jump host selected).

Deviations from the LLD:

- No `SshConfig::route()` iterator: `connect` takes the hops out of the config with
  `mem::take`, loops them, then connects the target with the same `connect_hop`; simpler than a
  borrowed `HopRef` while still moving each credential out before its hop authenticates.
- `authenticate` returns `Result<()>` (a server rejection is already the `Authentication` error)
  instead of `Result<AuthResult>`; callers never needed the success value.
- `JumpChainError` and `jump_chain(first, target)` replace `resolve_jump_chain(target)` so the
  same function serves the connect dialog (first = the saved reference, target = the session),
  Quick Connect (first = the picked session, no target), and the session dialog's Save check.
- A hop must have a saved username (`HopSpec::from_entry`); there is no per-hop username prompt.
- The jump-host picker is a GPUI Kit `Select` over `Vec<String>` rather than the searchable
  `Combobox` pattern of the Group field; it needs no custom trigger or footer.
- In Quick Connect the hop credential blocks are rebuilt inside the dialog's render closure when
  the picker selection changes (`QuickConnectHops::forms`); a chain error is shown as a red line
  under the picker and blocks Connect.
- `crates/ssh/src/agent.rs` lost its private `Agent` arm in `connect`; the arm lives in
  `route::authenticate` now.

Gaps:

- No real-host E2E: the shell through a bastion, `who` on the target, the hop-attributed
  host-key prompt text, `netstat` after close, a two-hop chain in the dialog, and Duplicate
  Session of a jump-host session are not exercised against real servers.
- `JumpHandles` drop order is asserted by construction (explicit drops in the teardown block),
  not by a test.
- Bandwidth indicator counts target-channel bytes only (documented out of scope).

## Handoff

Implemented, gate green, GUI walked, not committed. Next: owner acceptance, then US-0059 (port
forwarding), which will add the `open_rx` request arm to `ssh_main_task` beside the new
`jump_handles` parameter.
