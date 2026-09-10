# Work: SSH agent authentication

ID: US-0057
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
- Risk lane: high_risk (authentication; local agent access)
- Spec Intake, when required: IN-0023

## Outcome

A saved session or Quick Connect can choose **SSH agent** as its authentication method. Connect
finds the local agent (Windows OpenSSH named pipe, then Pageant; `$SSH_AUTH_SOCK` on Unix),
tries its identities in agent order until the server accepts one, and reports a clear error
when no agent is reachable, the agent is empty, or no identity is accepted. Nothing about the
agent or its keys is persisted.

## Scope

- [x] In scope: `SshAuthMethod::Agent` (`oneterm-core`); `crates/ssh/src/agent.rs` discovery
  + identity loop; the `Agent` arm in `connect` (the full `authenticate` helper extraction is
  left to US-0058, which owns the route loop); `SshAuthPreference::Agent` + serde in
  `oneterm-session-ui`; the radio in `SshAuthForm` for the session dialog, Quick Connect, and
  the connect dialog; Duplicate Session with agent auth opens the prefilled dialog with no
  credential field; unit tests with an in-process agent.
- [x] Out of scope: agent forwarding (US-0060); a settings entry for a custom agent socket;
  multi-method chaining after `partial_success`; key selection UI.

## Acceptance

- [x] Session dialog and Quick Connect offer Password / Private key / SSH agent; agent shows no credential field and saves `"auth_method": "agent"` (form + serde tests; GUI not walked).
- [ ] Connect with the agent holding one accepted key succeeds without any prompt (unit-proven against an in-process agent; manual Windows check pending).
- [x] With two keys where only the second is accepted, the second is used (one failure logged, no user-visible error).
- [ ] No agent: the notification says no agent is reachable and lists what was tried (message built in `no_agent_error`; not unit-tested because it needs a real missing pipe; manual check pending).
- [x] Empty agent: the notification says the agent holds no identities.
- [x] Six rejected identities: the notification names the server's remaining methods and "none of the 6 agent identities offered was accepted".
- [ ] RSA keys from the agent sign with `rsa-sha2-*` per `server-sig-algs` (never `ssh-rsa` SHA-1), as file keys do today (same `rsa_hash_alg` helper; RSA path not unit-tested, key generation is too slow for a unit test).
- [x] `ssh_session.json` files without the field still load as Password; no secret is written.
- [x] `pwsh scripts/ci-local.ps1` green.

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/high-level-design.md` — data flow step 4, crate placement.
- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/low-level-design/agent-auth.md` — discovery order, identity loop, interfaces, edge cases.
- `docs/ssh-authentication.md` — current methods, secret policy, "SSH agent authentication" listed under Out of Scope.
- `docs/decisions/0001-ssh-key-secret-persistence.md`, `docs/decisions/0002-ssh-duplicate-auth.md` — secrets in RAM only; duplicate prompts again (agent needs no prompt).
- `docs/agents/persistence.md` — `ssh_session.json` owner and additive-field rule.
- `docs/agents/dependencies.md` §3 — `russh` features; the agent client and `pageant` ship inside `russh` 0.61, no new workspace dependency.

### Documentation Action

- Update required: `docs/ssh-authentication.md` (add the SSH agent flow, discovery order, identity loop, error texts; remove it from Out of Scope; keep forwarding out until US-0060); `docs/agents/persistence.md` (`auth_method` gains `agent`); `docs/agents/structure.md` (`crates/ssh/src/agent.rs`).

Reason: the authentication contract enumerates the supported methods and their error texts; a new method changes that contract.

### Reconciliation

- `docs/ssh-authentication.md` — SSH Agent flow (discovery order, identity loop, cap, early stop, RSA hash, the three error texts), secret policy sentence, `auth_method` values, `crates/ssh/src/agent.rs` in Architecture; "SSH agent authentication" removed from Out of Scope, agent forwarding listed there until US-0060.
- `docs/agents/persistence.md` — `ssh_session.json` row names the `auth_method` values.
- `docs/agents/structure.md` — `agent.rs` and `test_support.rs` in the `ssh` crate tree.
- `docs/agents/dependencies.md` — `futures` as a dev-only dependency (in-process agent server for tests).
- `docs/decisions/0001`, `0002` — reviewed, unchanged: the agent method persists nothing and Duplicate Session still re-asks the agent.

## Context

- `authenticate_with_password` and the `PrivateKey` arm in `crates/ssh/src/session.rs` are the templates for phase handling (`phases.run(ConnectPhase::Authentication, ..)`) and the RSA hash choice (`rsa_hash_alg`, `best_supported_rsa_hash`).
- `russh::client::Handle::authenticate_publickey_with(user, PublicKey, Option<HashAlg>, &mut impl Signer)`; `AgentClient<R>` implements `Signer` (`russh/src/auth.rs`).
- `AgentClient::connect_named_pipe`, `connect_pageant` (Windows), `connect_env` (Unix) in `russh::keys::agent::client`; `russh::keys::agent::server` can serve a fake agent over `tokio::io::duplex` for tests.
- The keyboard-interactive tests in `crates/ssh/src/session.rs` already spawn an in-process `russh` server; reuse that harness.
- `SshAuthForm` (`crates/session-ui/src/auth_form.rs`) is shared by the three dialogs; `into_session` (`session_dialog.rs`) derives `key_path` from the preference.

## Plan

- [x] `SshAuthMethod::Agent` + `SshAuthPreference::Agent` (serde `agent`) with round-trip tests.
- [x] `crates/ssh/src/agent.rs`: `connect_agent`, `authenticate_with_agent`, `MAX_AGENT_IDENTITIES`.
- [x] `Agent` arm in `connect` (helper extraction deferred to US-0058).
- [x] In-process agent + server tests (accepted second key; empty agent; server drops publickey; six-identity cap). "No agent" is not unit-testable without a real missing pipe.
- [x] `SshAuthForm` radio + caption; `focus_handle` / `secret_focus_handle` return `Option` so an agent form focuses nothing; Duplicate Session prefills the agent radio.
- [x] Docs: `ssh-authentication.md`, `persistence.md`, `structure.md`, `dependencies.md`; gate green.

## Decisions

- DEC 0001, DEC 0002 (inherited). No new decision: discovery order follows OpenSSH for Windows and `SSH_AUTH_SOCK`.

## Verification Plan

- `cargo test -p oneterm-core -p oneterm-ssh -p oneterm-session-ui`
- `pwsh scripts/ci-local.ps1`
- Manual (Windows): `ssh-add` into the OpenSSH Authentication Agent service, connect with the agent radio; stop the service, connect again, read the error.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Commands (Windows 11, `pwsh`, branch `feat/ssh-agent-auth`, 2026-09-10):

- `rtk proxy cargo test -p oneterm-ssh agent_tests` — 4 passed, repeated six times after fixing a
  test that assumed the agent lists identities in insertion order (russh's agent server keeps a
  `HashMap`; the loop itself was right).
- `rtk proxy cargo test -p oneterm-session-ui` — 43 passed, including the new
  `agent_auth_round_trips_and_carries_no_key_path` and
  `agent_auth_has_no_input_to_focus_and_takes_agent_auth`.
- `cargo clippy -p oneterm-core -p oneterm-ssh -p oneterm-session-ui --all-targets -- -D warnings` — clean.
- `rtk proxy pwsh scripts/ci-local.ps1` — `ci-local: all checks passed` (fmt, clippy, workspace
  tests, dependency graph, doc paths, English check, completion catalogs, third-party notices).

Note for future runs: the `rtk` cargo filter summarised a run with one failing test as
"104 passed"; always run tests through `rtk proxy` and read the raw `test result` lines.

Deviations from the LLD:

- The `authenticate(handle, user, auth, phases)` extraction is not done; `connect` gained an
  `Agent` arm beside the existing three. US-0058 restructures that block into a per-hop loop
  and is the natural place for the extraction.
- `no_agent_error` names every candidate with its own error; on Windows the two candidates are
  the OpenSSH pipe and Pageant, on Unix `$SSH_AUTH_SOCK`.
- The session-test helpers (`TempKnownHosts`, `spawn_server`, `connect_trusting_loopback`) moved
  to `crates/ssh/src/test_support.rs` so the agent tests share them; the two per-server spawn
  functions in `session.rs` became one-liners over `spawn_server`.
- `futures` (0.3, already in `Cargo.lock` through GPUI) is a new dev-only dependency of
  `oneterm-ssh`: `russh::keys::agent::server::serve` takes a `Stream` of connections.

Gaps:

- Manual E2E on Windows (real `ssh-agent` service with `ssh-add`, a real server, the "no agent"
  and "agent stopped mid-session" notifications) was not run in this session; the OpenSSH
  agent service on this machine is `Stopped` / `Disabled` (`Get-Service ssh-agent`), so the "no agent" path is what a local trial would hit first.
- The RSA branch of the agent loop shares `rsa_hash_alg` with the file-key path but has no
  unit test (RSA key generation is too slow for the suite).
- Certificate identities are offered as their bare public key; a server that requires the
  certificate rejects them, which the loop reports like any other rejection.

## Handoff

Implemented, gate green, not committed. Next: owner acceptance (manual agent login on Windows),
then US-0058.
