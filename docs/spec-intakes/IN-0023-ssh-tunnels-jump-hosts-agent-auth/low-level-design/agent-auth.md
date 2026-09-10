# Low-Level Design: SSH agent authentication and forwarding bridge

Intake: IN-0023
HLD: ../high-level-design.md
Topic: agent-auth
Date: 2026-09-10

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`crates/ssh/src/agent.rs`: finding the local SSH agent, authenticating a hop with its
identities, and bridging a server-opened `auth-agent@openssh.com` channel back to it. Plus the
`Agent` variants in `oneterm-core` and `oneterm-session-ui`.

## Design

**Discovery.** `AgentClient<S>` is generic over its stream, so one boxed type serves every
platform:

```text
pub(crate) type AgentStream = Pin<Box<dyn AsyncReadWrite + Send>>;  // AsyncRead + AsyncWrite, blanket impl
pub(crate) async fn connect_agent() -> Result<AgentClient<AgentStream>, AgentError>
```

Order, first success wins, every failure collected into the error text:

| Platform | Candidates (in order) |
| --- | --- |
| Windows | `\\.\pipe\openssh-ssh-agent` (`AgentClient::connect_named_pipe`, also served by 1Password / gpg-agent emulation), then Pageant (`AgentClient::connect_pageant`) |
| Unix | `$SSH_AUTH_SOCK` (`AgentClient::connect_env`) |

No settings entry for a custom socket path in this intake (`SSH_AUTH_SOCK` already covers
every Unix agent; the Windows pipe name is fixed by OpenSSH). Error when nothing answers:
`No SSH agent is reachable (tried \\.\pipe\openssh-ssh-agent, Pageant)`.

**Identity loop** (runs inside `phases.run(ConnectPhase::Authentication, ..)`, so the 20 s
deadline and cancellation apply):

```text
let mut agent = connect_agent().await?;
let identities = agent.request_identities().await?;
if identities.is_empty() { return Err("The SSH agent holds no identities (run ssh-add)") }
let mut last_failure = None;
for identity in identities.iter().take(MAX_AGENT_IDENTITIES /* 6, OpenSSH MaxAuthTries */) {
    let hash_alg = if identity.pubkey.algorithm().is_rsa() {
        rsa_hash_alg(handle.best_supported_rsa_hash().await?)     // same helper as file keys
    } else { None };
    match handle.authenticate_publickey_with(user, identity.pubkey.clone(), hash_alg, &mut agent).await? {
        AuthResult::Success => return Ok(AuthResult::Success),
        AuthResult::Failure { remaining_methods, partial_success } => {
            last_failure = Some((remaining_methods, partial_success));
            if !remaining_methods.contains(MethodKind::PublicKey) { break }   // server stopped accepting publickey
        }
    }
}
Err(authentication_failure_message(..) + "; none of the N agent identities was accepted")
```

`best_supported_rsa_hash` is queried once and cached for the loop. A transport error in the
middle of the loop (server closed after too many tries) surfaces as the existing
`ConnectPhase::Authentication` error. The agent connection is dropped when the loop ends;
nothing agent-related is retained by the session unless forwarding is on.

**Forwarding request** (US-0060). After `channel_open_session`, when `cfg.agent_forwarding`:

```text
match channel.agent_forward(true).await {
    Ok(()) => log::info!("agent forwarding accepted"),
    Err(e) => emit warning notification "The server refused agent forwarding for <label>"; continue
}
```

Refusal is not a connect failure: `AllowAgentForwarding no` is common and the shell is still
useful. The notification is delivered through the session's existing event sink as a
`SessionEvent::Notice` (or the closest existing variant; see US-0060 packet for the exact hook).

**Forwarding bridge.** `SshClientHandler` gains `agent_forwarding: bool` and
`shutdown: CancellationToken`. In `server_channel_open_agent_forward(channel, _session)`:

```text
if !self.agent_forwarding { log::warn!("server opened an agent channel but forwarding is off"); drop(channel); return Ok(()) }
tokio::spawn(cancel_on(shutdown, async {
    let mut agent = connect_agent_stream().await?;          // raw stream, no AgentClient wrapper
    let mut remote = channel.into_stream();
    tokio::io::copy_bidirectional(&mut remote, &mut agent).await
}))
```

One fresh local agent connection per forwarded channel, so concurrent `ssh-add -l` and
`git fetch` on the remote host never share a pipe. Dropping the channel without accepting it
is the refusal: `russh` closes it and the remote `ssh` sees "agent refused operation".

**Core and persistence.**

```text
// oneterm-core
pub enum SshAuthMethod { None, Password { .. }, PrivateKey { .. }, Agent }
// oneterm-session-ui (persisted, serde snake_case)
pub enum SshAuthPreference { Password, PrivateKey, Agent }
pub struct SshSession { .., agent_forwarding: bool /* default false, skipped when false */ }
```

`SshAuthForm` shows no field for `Agent`; `into_session` requires no key path for it.
Duplicate Session with `Agent` needs no prompt (nothing to type), which is consistent with
DEC 0002: no secret is retained, the agent is asked again.

## Interfaces

```text
// crates/ssh/src/agent.rs
pub(crate) const MAX_AGENT_IDENTITIES: usize = 6;
pub(crate) async fn connect_agent() -> Result<AgentClient<AgentStream>, AgentError>;
pub(crate) async fn connect_agent_stream() -> Result<AgentStream, AgentError>;
pub(crate) async fn authenticate_with_agent(
    handle: &mut Handle<SshClientHandler>, user: &str,
) -> Result<AuthResult, AppError>;
pub(crate) fn spawn_agent_bridge(channel: Channel<Msg>, shutdown: CancellationToken);

// crates/ssh/src/handler.rs
impl SshClientHandler { pub(crate) fn new(host, port, policy, agent_forwarding: bool, shutdown: CancellationToken) -> Self }
```

## Edge Cases and Failure Modes

- [ ] No agent running: authentication fails with the "not reachable" message listing what was tried; no retry.
- [ ] Agent running, zero identities: explicit "holds no identities" error.
- [ ] Agent has 10 identities, the 7th is the right one: the loop stops at 6 with the "none of the 6 tried" message (matches OpenSSH behaviour; the user removes stale keys with `ssh-add -d`).
- [ ] Server answers `partial_success` (two-factor: publickey then password): reported through the existing failure message naming the remaining methods; multi-method chaining stays out of scope (`docs/ssh-authentication.md`).
- [ ] Agent prompts for confirmation (`ssh-add -c`) or a hardware token touch: the 20 s authentication deadline covers it; on timeout the phase error names the deadline as today.
- [ ] Agent forwarding on, local agent gone mid-session: the bridge task fails per channel, the remote `ssh` sees a refused agent, the terminal session is unaffected.
- [ ] Server opens an agent channel while forwarding is off (misbehaving or malicious server): dropped and logged at warn; never bridged.
- [ ] Cancellation during the identity loop: `phases.run` observes the token between attempts; a pending `sign_request` is abandoned when the future drops.

## Verification

- [ ] `cargo test -p oneterm-ssh`: in-process agent via `russh::keys::agent::server::serve` over a `tokio::io::duplex` pair (holding two keys, first rejected by the test server, second accepted) + the in-process `russh` server used by the keyboard-interactive tests, asserting `Success` on the second identity and that a server rejecting `publickey` ends the loop after one attempt.
- [ ] Same harness: zero identities → error text; six rejected identities → "none of the 6" text.
- [ ] Handler test: `server_channel_open_agent_forward` with `agent_forwarding = false` closes the channel without opening a local agent connection (mock `connect_agent_stream` not called).
- [ ] `cargo test -p oneterm-session-ui`: `ssh_session.json` round-trip with `"auth_method": "agent"` and `"agent_forwarding": true`; a v2 file without the fields loads as password + false.
- [ ] Manual (Windows): `ssh-add` a key into the OpenSSH agent service, connect with the SSH agent radio; on the remote host `ssh-add -l` lists the key only when the session has forwarding on.
