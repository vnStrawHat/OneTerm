# Work: Port forwarding tunnels

ID: US-0059
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
- Risk lane: high_risk (external effects: listening sockets on the local machine and on the remote host)
- Spec Intake, when required: IN-0023

## Outcome

A saved session lists port forwards. While it is connected, every `Local` forward accepts
connections on the local machine and relays them through the session; every `Dynamic` forward
is a local SOCKS5 (no-auth, CONNECT) proxy through the session; every `Remote` forward makes
the server listen and relays incoming connections to a local target. A forward that cannot
start produces one warning and the shell still opens. Closing the session frees every port.

## Scope

- [x] In scope: `PortForward` + `validate` + serde (`oneterm-core`); `crates/ssh/src/tunnel.rs`
  (listeners, SOCKS5 subset, relays, `HandleRequest`, forward table); the `open_rx` arm in
  `ssh_main_task`; `server_channel_open_forwarded_tcpip` in the handler; teardown through the
  session token; `SshSession.port_forwards` + dialog rows; bind-failure warnings; tests.
- [x] Out of scope: SOCKS4/4a, SOCKS5 authentication and UDP; a status-bar or tab indicator
  for active tunnels (follow-up packet if wanted); editing forwards on a live session
  (reconnect applies changes); Quick Connect forwards.

## Acceptance

- [x] Session dialog has an "Add" button and one row per forward (kind, bind, port, target host, target port); Dynamic hides the target; invalid rows and duplicate `(kind, bind, port)` are rejected on Save (rows walked in the GUI; validation and the duplicate check in `PortForwardRows::take`, `PortForward::validate` unit-tested).
- [ ] Local: `curl http://127.0.0.1:8080` reaches a remote `http.server` on port 80 through the session (unit-proven: bytes echo through a local forward over an in-process server; no real host).
- [x] Dynamic: SOCKS5 greeting + CONNECT + echo, BIND answered `07`, user/pass-only client refused with `05 FF` (unit-tested; `curl --socks5` against a real host not run).
- [x] Remote: a server-opened forwarded-tcpip channel reaches a local listener both ways; a channel for a port never requested is dropped (unit-tested with russh's server handle; no real sshd).
- [x] Bind port in use: one `SessionEvent::Notification` naming the address; the other forward still starts (unit-tested; the toast itself not walked).
- [x] Closing the tab frees every local port: cancelling the session token releases the bound listener within the test; `ssh_main_task` cancels that token in its teardown block.
- [ ] Duplicate Session of a session with a local forward opens with the bind warning and a working shell (`SshDuplicateConfig::port_forwards` is carried and passed by the duplicate dialog; not walked).
- [x] `ssh_session.json` without `port_forwards` loads unchanged (round-trip test; empty list omitted on save).
- [x] `pwsh scripts/ci-local.ps1` green.

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

- `docs/ssh-client-connect.md` — new §9.9 "Port forwards": start point, the `HandleRequest` path, the SOCKS5 subset, remote forwards through the handler, the session token, duplicates.
- `docs/terminal-backend.md` — teardown bullet names `session_shutdown` and the listeners; new bullet for the third `select!` arm.
- `docs/agents/persistence.md` — `port_forwards` field.
- `docs/agents/structure.md` — `crates/core/src/port_forward.rs`, `crates/ssh/src/tunnel.rs`, `crates/session-ui/src/forward_rows.rs`.
- `docs/sftp-browser-design.md` — reviewed, unchanged: the "handle is moved into ssh_main_task" precedent still holds; forwards route their opens through the task instead of moving the handle.
- `docs/agents/dependencies.md` — reviewed, unchanged: `thiserror` was already in the "Errors and logs" group; `oneterm-ssh` merely starts using it. No SOCKS crate.

## Context

- `Handle::channel_open_direct_tcpip`, `Handle::tcpip_forward(address, port) -> Result<bool>`, `Handler::server_channel_open_forwarded_tcpip(channel, connected_address, connected_port, originator_address, originator_port, session)` in `russh` 0.61.
- `open_sftp` (`crates/ssh/src/session.rs`) is the precedent for work done on the handle before `ssh_main_task` is spawned and for the shared `CancellationToken`.
- `SessionEventSink` (`crates/terminal`) is the path for the warning notification; check which variant the keepalive disconnect notice uses and reuse it.
- `tokio::io::copy_bidirectional` for relays; `tokio::net::TcpListener` / `TcpStream` are available (`net` feature).

## Plan

- [x] `port_forward.rs` in core: types, `validate`, `summary`, `bind_key`, `binds_loopback`, serde tests.
- [x] `tunnel.rs`: `HandleRequest` + `serve_handle_request`, `start_forwards`, accept loop + relay, `socks5_connect_target`, `spawn_forwarded_tcpip`, `ForwardTable`; `open_rx` arm in `task.rs`; `SshClientHandler::with_forwards` + `server_channel_open_forwarded_tcpip`; token renamed `session_shutdown`.
- [x] In-process server tests: local echo, SOCKS5 echo + `07` + `FF`, remote forward both ways + unknown port dropped, bind failure warns, cancel frees the port.
- [x] `SshSession.port_forwards`, `forward_rows.rs` (rows, Add/remove, parse, duplicate check, non-loopback caution), session dialog widened to 560 px; `SshDuplicateConfig::port_forwards`.
- [x] Docs + gate + GUI walk (`evidence/US-0059-gui-walk.md`). Manual `curl` checks not run (no host).

## Decisions

- DEC-0011 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual (Windows): the three `curl` checks above; `netstat -an | findstr LISTEN` before and after closing the tab.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/ssh-port-forwarding`, 2026-09-10):

- `rtk proxy cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui` — 50 + 63 + 49
  passed; the five tunnel tests run an in-process `russh` server that echoes direct-tcpip
  channels and opens forwarded-tcpip channels from its server handle (repeated three times
  after the SOCKS5 fix below).
- `rtk proxy cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`, 1100 tests.
- GUI walk: `evidence/US-0059-gui-walk.md` (one screenshot of the forward rows).

Found by the tests: closing a socket with unread bytes makes Windows send a reset, so the
first SOCKS5 implementation lost its `07` reply for BIND; the request is now read completely
before the command is judged, and refusals shut the socket down explicitly.

Deviations from the LLD:

- `start_forwards` returns the bound local addresses (tests bind port 0), and the accepted
  remote listener port replaces a requested 0 in the table, as the LLD describes.
- Warnings go through a clone of the session event sender as `SessionEvent::Notification`
  (the OSC 9 toast path) rather than a new sink variant.
- `SshClientHandler::with_forwards` keeps `new` unchanged; only the target's handler carries
  the table, jump hops pass `None`.
- The session dialog grew to 560 px so one forward fits on a line; `FormDialog` has no scroll,
  which is recorded as a `ponytail:` ceiling (about five rows on a 1080p work area).

Gaps:

- No real-host E2E: `curl` through the three forward kinds, the toast in a live terminal,
  `netstat` after closing the tab, and Duplicate Session with a taken port.
- No status indicator for active tunnels (out of scope by the packet; DEC-0011 follow-up).
- SOCKS5 handshake failures after a partial read are logged at debug only.

## Handoff

Implemented, gate green, GUI walked, not committed. Next: owner acceptance, then US-0060
(agent forwarding), which reuses `connect_agent_stream` and the `session_shutdown` token.
