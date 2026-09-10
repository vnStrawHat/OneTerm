# Work: Port forwarding tunnels

ID: US-0059
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
- Risk lane: high_risk (external effects: listening sockets on the local machine and on the remote host)
- Spec Intake, when required: IN-0023

## Outcome

A saved session lists port forwards. While it is connected, every `Local` forward accepts
connections on the local machine and relays them through the session; every `Dynamic` forward
is a local SOCKS5 (no-auth, CONNECT) proxy through the session; every `Remote` forward makes
the server listen and relays incoming connections to a local target. A forward that cannot
start produces one warning and the shell still opens. Closing the session frees every port.

## Scope

- [ ] In scope: `PortForward` + `validate` + serde (`oneterm-core`); `crates/ssh/src/tunnel.rs`
  (listeners, SOCKS5 subset, relays, `HandleRequest`, forward table); the `open_rx` arm in
  `ssh_main_task`; `server_channel_open_forwarded_tcpip` in the handler; teardown through the
  session token; `SshSession.port_forwards` + dialog rows; bind-failure warnings; tests.
- [ ] Out of scope: SOCKS4/4a, SOCKS5 authentication and UDP; a status-bar or tab indicator
  for active tunnels (follow-up packet if wanted); editing forwards on a live session
  (reconnect applies changes); Quick Connect forwards.

## Acceptance

- [ ] Session dialog has an "Add" button and one row per forward (kind, bind, port, target host, target port); Dynamic hides the target; invalid rows and duplicate `(kind, bind, port)` are rejected on Save.
- [ ] Local: `curl http://127.0.0.1:8080` reaches a remote `http.server` on port 80 through the session.
- [ ] Dynamic: `curl --socks5 127.0.0.1:1080 http://example.com` succeeds; a BIND request gets reply `07`; a client offering only user/pass auth is refused with `05 FF`.
- [ ] Remote: on the server, `curl http://127.0.0.1:9000` reaches a local service; a forwarded channel for a port OneTerm never requested is dropped.
- [ ] Bind port in use: one warning toast naming the address; the shell opens; other forwards still start.
- [ ] Closing the tab frees every local port (`netstat -an` shows no LISTEN on 8080/1080) and stops every relay.
- [ ] Duplicate Session of a session with a local forward opens with the bind warning and a working shell.
- [ ] `ssh_session.json` without `port_forwards` loads unchanged.
- [ ] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/high-level-design.md` — data flow steps 6, 7, 8; wireframes.
- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/low-level-design/port-forwarding.md` — spec, listeners, SOCKS5 table, remote callback, teardown.
- `docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — loopback default, warn-not-abort, SOCKS5 subset.
- `docs/terminal-backend.md` §SSH — `ssh_main_task` select loop, single teardown block, `sftp_shutdown` token.
- `docs/sftp-browser-design.md` "Handle is moved into ssh_main_task" — the precedent for opening channels before spawn and the reason for `HandleRequest`.
- `docs/ssh-client-connect.md` — connect phases; forwards start after `ShellRequest`, never a phase of their own.
- `docs/agents/persistence.md` — `ssh_session.json` additive field.
- `docs/agents/dependencies.md` §3 — `tokio` already has `net`; no SOCKS crate.

### Documentation Action

- Update required: `docs/ssh-client-connect.md` (forwards section: start order, request channel, teardown); `docs/terminal-backend.md` (`ssh_main_task` gains the `open_rx` arm; token renamed `session_shutdown`); `docs/agents/persistence.md` (`port_forwards`); `docs/agents/structure.md` (`crates/core/src/port_forward.rs`, `crates/ssh/src/tunnel.rs`).

Reason: the backend document describes the main task's arms and teardown precisely; both change.

### Reconciliation

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `Handle::channel_open_direct_tcpip`, `Handle::tcpip_forward(address, port) -> Result<bool>`, `Handler::server_channel_open_forwarded_tcpip(channel, connected_address, connected_port, originator_address, originator_port, session)` in `russh` 0.61.
- `open_sftp` (`crates/ssh/src/session.rs`) is the precedent for work done on the handle before `ssh_main_task` is spawned and for the shared `CancellationToken`.
- `SessionEventSink` (`crates/terminal`) is the path for the warning notification; check which variant the keepalive disconnect notice uses and reuse it.
- `tokio::io::copy_bidirectional` for relays; `tokio::net::TcpListener` / `TcpStream` are available (`net` feature).

## Plan

- [ ] `port_forward.rs` in core: types, `validate`, `summary`, serde tests.
- [ ] `tunnel.rs`: `HandleRequest`, `start_forwards`, local accept loop + relay, `socks5_connect_target`, `spawn_forwarded_tcpip`, forward table; `open_rx` arm in `task.rs`; handler callback; rename the token.
- [ ] In-process server tests: local echo, SOCKS5 echo + `07` + `FF`, remote forward to a test listener, unknown port dropped, cancel frees port.
- [ ] `SshSession.port_forwards`, dialog rows, duplicate check, bind caution text; warning wiring.
- [ ] Docs + gate; manual `curl` checks.

## Decisions

- DEC-0011 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual (Windows): the three `curl` checks above; `netstat -an | findstr LISTEN` before and after closing the tab.

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

Not started. Independent of US-0057/US-0058; touches `task.rs` and `handler.rs`, so rebase on whichever of those lands first.
