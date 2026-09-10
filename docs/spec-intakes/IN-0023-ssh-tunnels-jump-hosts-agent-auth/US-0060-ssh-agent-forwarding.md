# Work: SSH agent forwarding

ID: US-0060
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

- [ ] In scope: `SshConfig.agent_forwarding` (`oneterm-core`); `channel.agent_forward(true)`
  after `channel_open_session`; `SshClientHandler { agent_forwarding, shutdown }` and
  `server_channel_open_agent_forward` bridge (`spawn_agent_bridge`); refusal warning;
  `SshSession.agent_forwarding` + the checkbox in the session dialog; Duplicate Session
  copies the switch; tests.
- [ ] Out of scope: Quick Connect switch; forwarding through jump hops to the target only
  (the request is made on the target's session channel; hops never see an agent channel);
  a global "always forward" setting; key-use confirmation prompts (that is the agent's job,
  `ssh-add -c`).

## Acceptance

- [ ] Session dialog checkbox, unchecked by default; saved as `"agent_forwarding": true` only when on.
- [ ] With the switch on and a key in the local agent: `ssh-add -l` on the remote host lists the key; `git fetch` from a private repository on the remote host succeeds without a remote key.
- [ ] With the switch off: `ssh-add -l` on the remote host reports no agent; a test server that opens an agent channel anyway sees it closed and OneTerm never connects to the local agent.
- [ ] Server with `AllowAgentForwarding no`: one warning toast naming the session; the shell opens.
- [ ] Local agent stopped mid-session: the remote `ssh` sees a refused agent; the terminal keeps working.
- [ ] Works with Password, Private key, and SSH agent authentication alike (the switch is independent of the auth method).
- [ ] `pwsh scripts/ci-local.ps1` green.

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

Before completion, list docs changed or confirm the recorded no-change reason remains valid.

## Context

- `Channel::agent_forward(want_reply)` (`russh/src/channels/mod.rs`); `Handler::server_channel_open_agent_forward(channel, session)` (`russh/src/client/mod.rs`).
- `connect_agent_stream` from US-0057 (`crates/ssh/src/agent.rs`) gives the raw local agent stream for `copy_bidirectional`.
- `SshClientHandler::new` is called once per hop in US-0058; only the target's handler gets `agent_forwarding = cfg.agent_forwarding`, hops get `false`.
- The warning uses the same sink path as the bind-failure warning of US-0059 (or the keepalive notice if US-0059 has not landed).

## Plan

- [ ] `SshConfig.agent_forwarding`, `SshSession.agent_forwarding` (serde default false) + round-trip test.
- [ ] Handler flag + `server_channel_open_agent_forward` + `spawn_agent_bridge`; request after `channel_open_session`; refusal warning.
- [ ] Tests: server opens an agent channel with the flag off (closed, agent untouched); with the flag on, bytes flow to a fake agent over duplex; request refused yields the warning event.
- [ ] Dialog checkbox; Duplicate Session copies it.
- [ ] Docs + gate; manual `ssh-add -l` on the remote host on/off.

## Decisions

- DEC-0011 (proposed; must be accepted before implementation).

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual (Windows + Linux host): `ssh-add -l` on the remote host with the switch on and off; `AllowAgentForwarding no` on the test server for the refusal toast.

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

Not started. Depends on US-0057 (`connect_agent_stream`).
