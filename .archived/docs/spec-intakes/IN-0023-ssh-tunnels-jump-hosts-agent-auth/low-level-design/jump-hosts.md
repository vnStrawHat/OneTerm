# Low-Level Design: Jump hosts and the connection route

Intake: IN-0023
HLD: ../high-level-design.md
Topic: jump-hosts
Date: 2026-09-10

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`crates/ssh/src/route.rs`: opening the transport for each hop, authenticating it, keeping the
hop handles alive, and attributing host-key prompts and errors to the right hop. Plus the
saved-session reference, chain resolution, and per-hop credential collection in
`crates/session-ui`.

## Design

**Core types.**

```text
pub struct SshHop {
    pub host: String, pub port: u16, pub username: String,
    pub auth: SshAuthMethod, pub host_key_policy: HostKeyPolicy,
}
pub struct SshConfig { .., pub jump_hops: Vec<SshHop> /* client-side first */, .. }
pub const MAX_JUMP_HOPS: usize = 4;
```

The target keeps its fields flat on `SshConfig` (no churn for existing callers); a hop is the
same five fields. `SshConfig::route(&self) -> impl Iterator<Item = HopRef>` yields hops then
target so the backend has one loop.

**Backend walk** (`connect`, replacing the single `client::connect` call):

```text
let mut carriers: Vec<Handle<SshClientHandler>> = Vec::new();   // authenticated hops
for (index, hop) in cfg.route().enumerate() {
    let handler = SshClientHandler::new(hop.host, hop.port, hop.host_key_policy, ..);
    let client_cfg = /* keepalive + preferred key algorithms, as today */;
    let mut handle = phases.run(Transport, async {
        match carriers.last() {
            None => client::connect(client_cfg, (hop.host, hop.port), handler).await,
            Some(prev) => {
                let ch = prev.channel_open_direct_tcpip(hop.host, hop.port, "127.0.0.1", 0).await?;
                client::connect_stream(client_cfg, ch.into_stream(), handler).await
            }
        }
    }.map_err(|e| hop_error(index, hop, e))).await?;
    let result = phases.run(Authentication, authenticate(&mut handle, hop.username, hop.auth)).await
        .map_err(|e| hop_error(index, hop, e))?;
    if let Failure { .. } = result { return Err(hop_error(index, hop, auth failure)) }
    carriers.push(handle);
}
let handle = carriers.pop();                 // the target
let jump_handles = JumpHandles(carriers);    // moved into ssh_main_task as `_jump_handles`
```

`authenticate` is the existing inline `match` on `SshAuthMethod` extracted into a function
(the `Agent` arm is `agent-auth.md`). The authentication material of each hop is taken out of
the config with `std::mem::replace` before the call, exactly as the target's is today, so every
secret is zeroized once its hop is authenticated.

**Error attribution.** `hop_error` leaves `AppError::Connect { phase, message }` and the
host-key variants (`UnknownHostKey`, `HostKeyChanged`) unchanged in kind — those already carry
`host` and `port`, which is enough for the UI to find the hop — and prefixes the message of
every other error with `jump host <user>@<host>:<port>: ` when `index < jump_hops.len()`.

**Handle lifetime.** `ssh_main_task` receives `_jump_handles: JumpHandles`. In the teardown
block, after the target channel close and the target `disconnect`, `JumpHandles` is dropped;
its `Drop` pops handles from the end (closest to the target) to the front, so no hop's TCP
connection closes while a later hop still rides on it. Keepalives run per hop because each
`Handle` has its own `russh` config.

**Session UI.**

```text
pub struct SshSession { .., #[serde(default, skip_serializing_if = "Option::is_none")] pub jump_host: Option<SshSessionId> }

pub enum JumpChainError { Missing(SshSessionId), Cycle(SshSessionId), TooLong(usize) }
impl SshSessionStore {
    /// Hops from the client outwards, excluding `target` itself.
    pub fn resolve_jump_chain(&self, target: SshSessionId) -> Result<Vec<SshSessionEntry>, JumpChainError>;
}
```

`resolve_jump_chain` walks `jump_host` links with a visited set; it stops with `Cycle` when an
id repeats (including `target`) and `TooLong` beyond `MAX_JUMP_HOPS`; the resulting vector is
reversed so hop 0 is the outermost. The session dialog's jump-host combobox lists every other
saved session as `label (user@host:port)`; picking one whose own chain would include the
edited session is rejected on Save with the same error text.

Connect dialog: `SshAuthForm` becomes one block per route entry — `Vec<(SharedString, SshAuthForm)>`
built from the chain plus the target — rendered as in the HLD wireframe. On Connect each
block yields its `SshAuthMethod`; the hops go into `SshHop { auth, host_key_policy: Strict, .. }`
and the target into `cfg.auth`. Fields are cleared after submission as today. A hop with
`Agent` renders a single informational line and yields `SshAuthMethod::Agent`.

Host-key confirmation: `open_host_key_confirmation` receives the failing `host:port` from the
error; it sets `AcceptNewFingerprint` on the matching hop (or on the target when none matches)
and adds `(jump host for <target label>)` to the dialog text for a hop. The retry reuses the
short-lived zeroizing config clone that the existing flow already keeps for this purpose; with
hops it holds the whole route's auth set for that single retry.

Duplicate Session (DEC 0002): the duplicate carries the resolved chain's non-secret metadata
and prompts for every hop again. Quick Connect: the jump-host combobox stores nothing unless
"Save to SSH Sessions" is ticked.

## Interfaces

```text
// oneterm-core
pub struct SshHop { .. }                                   // see above
impl SshConfig { pub fn route(&self) -> impl Iterator<Item = HopRef<'_>>; }
pub const MAX_JUMP_HOPS: usize = 4;

// oneterm-ssh (crate-private)
pub(crate) async fn open_transport(prev: Option<&Handle<SshClientHandler>>, hop: HopRef<'_>, client_cfg: Arc<client::Config>, handler: SshClientHandler) -> Result<Handle<SshClientHandler>, AppError>;
pub(crate) async fn authenticate(handle: &mut Handle<SshClientHandler>, user: &str, auth: SshAuthMethod, phases: &ConnectPhases) -> Result<AuthResult, AppError>;
pub(crate) struct JumpHandles(Vec<Handle<SshClientHandler>>);   // Drop pops from the back
pub(crate) fn hop_error(index: usize, hop: HopRef<'_>, error: AppError) -> AppError;

// oneterm-session-ui
impl SshSessionStore { pub fn resolve_jump_chain(&self, target: SshSessionId) -> Result<Vec<SshSessionEntry>, JumpChainError>; }
```

## Edge Cases and Failure Modes

- [ ] Jump host deleted after the reference was saved: `Missing(id)` at connect time, shown as a notification; the session dialog shows the combobox as "None (missing session)" and Save clears the field.
- [ ] A -> B -> A: `Cycle`, rejected on Save and on Connect.
- [ ] Five hops: `TooLong(5)`, rejected on Save and on Connect.
- [ ] Hop authenticates but the direct-tcpip open to the next hop is refused (sshd `AllowTcpForwarding no` on the bastion): `Transport` phase error prefixed with the hop, naming the next hop's host:port.
- [ ] Unknown host key on hop 1: the unknown-host error names hop 1; the confirmation dialog says "(jump host for <label>)"; the retry marks only hop 1 as accepted and re-checks every other hop strictly.
- [ ] Host key changed on any hop: fail closed, as today, with the hop named.
- [ ] Cancel while hop 1 is authenticating: `phases.run` returns cancelled; the already-open hop 0 handle is dropped by the error path (connection closed).
- [ ] Bandwidth indicator: counts target-channel bytes as today; jump-hop overhead is not counted.
- [ ] Keepalive: each hop's `russh` config carries the same keepalive settings; a dead hop closes the hops above it and the session ends with the existing disconnect notice.

## Verification

- [ ] `cargo test -p oneterm-core`: `route()` order (hops then target); `MAX_JUMP_HOPS`.
- [ ] `cargo test -p oneterm-ssh`: two in-process `russh` servers, the first allowing direct-tcpip to the second; `connect` with one hop opens a shell on the second and both handles stay alive until `close()`; the first server refusing direct-tcpip yields a `Transport` error prefixed with `jump host`; an unknown host key on the hop yields `UnknownHostKey` with the hop's host and port.
- [ ] `cargo test -p oneterm-session-ui`: `resolve_jump_chain` for a two-hop chain, a missing id, a cycle, and a five-hop chain; `ssh_session.json` round-trip with `jump_host`; the dialog rejects self-reference.
- [ ] Manual: connect to a real host through a real bastion, confirm one host-key prompt per unknown hop, and confirm `who` on the target shows the bastion's address as the origin.
