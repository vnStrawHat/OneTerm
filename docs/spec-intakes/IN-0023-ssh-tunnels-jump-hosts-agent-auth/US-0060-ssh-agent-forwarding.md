# Work: SSH agent forwarding

ID: US-0060
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
- Risk lane: high_risk (the remote host can sign with local keys while the session is open)
- Spec Intake, when required: IN-0023

## Outcome

A saved session has a "Forward the SSH agent to the remote host" switch, off by default.
When on, the session requests agent forwarding after the session channel opens; every agent
channel the server then opens is bridged to a fresh connection to the local agent, so `ssh`
and `git` on the remote host can use the local keys. When off, an agent channel opened by the
server is closed immediately. A server that refuses the request produces one warning and the
shell still opens.

## Scope

- [x] In scope: `SshConfig.agent_forwarding` (`oneterm-core`); `channel.agent_forward(true)`
  after `channel_open_session`; `SshClientHandler { agent_forwarding, shutdown }` and
  `server_channel_open_agent_forward` bridge (`spawn_agent_bridge`); refusal warning;
  `SshSession.agent_forwarding` + the checkbox in the session dialog; Duplicate Session
  copies the switch; tests.
- [x] Out of scope: Quick Connect switch; forwarding through jump hops to the target only
  (the request is made on the target's session channel; hops never see an agent channel);
  a global "always forward" setting; key-use confirmation prompts (that is the agent's job,
  `ssh-add -c`).

## Acceptance

- [x] Session dialog checkbox, unchecked by default; saved as `"agent_forwarding": true` only when on (GUI walked; serde test).
- [ ] With the switch on and a key in the local agent: `ssh-add -l` on the remote host lists the key (unit-proven: a server-opened agent channel round-trips bytes through the injected local agent connector; no real host).
- [x] With the switch off: a test server that opens an agent channel anyway sees it closed and the local agent connector is never called.
- [x] Server with `AllowAgentForwarding no`: `request_agent_forwarding` returns `Ok(false)` (unit-tested against a refusing server) and `connect` turns it into one `SessionEvent::Notification` naming `user@host:port`; the toast itself not walked.
- [ ] Local agent stopped mid-session: the bridge task logs the connector error and ends; the terminal is untouched (by construction; not exercised).
- [x] Works with Password, Private key, and SSH agent authentication alike: the switch lives on `SshConfig`, not on `SshAuthMethod` (the tests authenticate with a password).
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/high-level-design.md` — data flow step 5, step 7 (agent channel), wireframe checkbox.
- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/low-level-design/agent-auth.md` — forwarding request, bridge, refusal.
- `docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — off by default, per session, handler refuses when off.
- `docs/ssh-authentication.md` — agent section written by US-0057; gains the forwarding paragraph.
- `docs/ssh-client-connect.md` — where the request sits between `ChannelOpen` and `PtyRequest`.
- `docs/agents/persistence.md` — additive field.

### Documentation Action

- Update required: `docs/ssh-authentication.md` (agent forwarding: request point, bridge, refusal, security note); `docs/ssh-client-connect.md` (request order); `docs/agents/persistence.md` (`agent_forwarding`).

Reason: the authentication contract's security policy must state what the remote host can do with the local agent and that it is opt-in per session.

### Reconciliation

- `docs/ssh-authentication.md` — "Agent forwarding" bullet (switch, request point, bridge, refusal, off-by-default refusal of server-opened channels, security note); the Out of Scope entry for forwarding removed.
- `docs/ssh-client-connect.md` — §9.9 names the request point between `ChannelOpen` and `PtyRequest` and the handler bridge.
- `docs/agents/persistence.md` — `agent_forwarding` field.
- `docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — reviewed, unchanged: implemented as decided (per session, off by default, handler closes unrequested channels).
- `docs/agents/structure.md` — reviewed, unchanged: no new file; the bridge lives in `agent.rs`.

## Context

- `Channel::agent_forward(want_reply)` (`russh/src/channels/mod.rs`); `Handler::server_channel_open_agent_forward(channel, session)` (`russh/src/client/mod.rs`).
- `connect_agent_stream` from US-0057 (`crates/ssh/src/agent.rs`) gives the raw local agent stream for `copy_bidirectional`.
- `SshClientHandler::new` is called once per hop in US-0058; only the target's handler gets `agent_forwarding = cfg.agent_forwarding`, hops get `false`.
- The warning uses the same sink path as the bind-failure warning of US-0059 (or the keepalive notice if US-0059 has not landed).

## Plan

- [x] `SshConfig.agent_forwarding`, `SshDuplicateConfig.agent_forwarding`, `SshSession.agent_forwarding` (serde default false, omitted when false) + round-trip test.
- [x] `SshClientHandler::with_agent_forwarding(connector, token)` + `server_channel_open_agent_forward` + `agent::spawn_agent_bridge`; `agent::request_agent_forwarding` after `channel_open_session` (waits for the channel reply); refusal notification. `connect_hop` now takes the prebuilt handler so only the target's carries forwards and the bridge.
- [x] Tests: refused request is `Ok(false)`; accepted request bridges a server-opened agent channel to an in-memory echo agent exactly once; without the switch the channel is closed and the connector never called.
- [x] Dialog checkbox ("Forward the SSH agent to the remote host"); the duplicate dialog passes the flag through.
- [x] Docs + gate + GUI walk. Manual `ssh-add -l` on a remote host not run (no host).

## Decisions

- DEC-0011 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual (Windows + Linux host): `ssh-add -l` on the remote host with the switch on and off; `AllowAgentForwarding no` on the test server for the refusal toast.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/ssh-agent-forwarding`, 2026-09-10):

- `rtk proxy cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui` — 50 + 66 + 51
  passed; the three new agent-forwarding tests run an in-process server whose `agent_request`
  answers success or failure and then opens agent channels from its server handle (repeated
  three times).
- `rtk proxy cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed`, 1105 tests.
- GUI walk: `evidence/US-0060-gui-walk.md`.

Found by the tests: dropping a `russh` channel on the client does not tell the server; the
handler now calls `channel.close()` on an unrequested agent channel (and, the same way, on an
unrequested forwarded-tcpip channel from US-0059) so the server sees `Close`.

Deviations from the LLD:

- The bridge takes an injectable `AgentConnector` (a boxed async factory) instead of calling
  `connect_agent_stream` directly, so the tests can hand it an in-memory agent; production
  passes `local_agent_connector()`.
- `request_agent_forwarding` waits for the channel `Success` / `Failure` itself because
  `russh` channel requests do not return their reply; it runs under the `ChannelOpen` phase
  deadline.
- No `agent_forwarding` on `SshHop`: hops never carry the switch, matching the packet scope.

Gaps:

- No real-host E2E: `ssh-add -l` / `git fetch` on a remote host, the refusal toast in a live
  terminal, and stopping the local agent mid-session.
- Quick Connect has no switch (scope); a duplicate of a forwarding session forwards too.

## Handoff

Implemented, gate green, GUI walked, not committed. This closes the last packet of IN-0023;
next is owner acceptance and merging the four `feat/ssh-*` branches into `main`.
