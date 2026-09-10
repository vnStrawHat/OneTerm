# Low-Level Design: Port forwarding (local, remote, dynamic)

Intake: IN-0023
HLD: ../high-level-design.md
Topic: port-forwarding
Date: 2026-09-10

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

`crates/ssh/src/tunnel.rs`: the `PortForward` spec, local and dynamic listeners, the SOCKS5
subset, remote forwards through the handler callback, the request channel to the handle
owner, and teardown. Plus the saved list and dialog rows in `crates/session-ui`.

## Design

**Spec** (`crates/core/src/port_forward.rs`, serde snake_case, persisted by `session-ui`):

```text
pub enum PortForward {
    Local   { bind: IpAddr /* default 127.0.0.1 */, bind_port: u16, target_host: String, target_port: u16 },
    Remote  { bind_host: String /* default "127.0.0.1" */, bind_port: u16, target_host: String, target_port: u16 },
    Dynamic { bind: IpAddr /* default 127.0.0.1 */, bind_port: u16 },
}
impl PortForward {
    pub fn validate(&self) -> Result<(), PortForwardError>;   // ports 1..=65535, non-empty hosts, no whitespace
    pub fn summary(&self) -> String;                          // "L 127.0.0.1:8080 -> localhost:80"
}
```

`Remote.bind_host` is a string because sshd interprets `""`, `"*"`, `"localhost"` specially
(`GatewayPorts`); the dialog default is `127.0.0.1`.

**Handle access.** `russh::client::Handle` is owned by `ssh_main_task`. A bounded mpsc
`HandleRequest` channel (capacity 32) is created in `connect` and its receiver moved into the
task, which gains one `select!` arm:

```text
pub(crate) enum HandleRequest {
    OpenDirectTcpip { host: String, port: u32, originator: SocketAddr, reply: oneshot::Sender<Result<Channel<Msg>, russh::Error>> },
}
// in ssh_main_task select!:
Some(req) = open_rx.recv() => match req { OpenDirectTcpip { .. } => { let r = handle.channel_open_direct_tcpip(host, port, originator.ip(), originator.port()).await; let _ = reply.send(r); } }
```

The open awaits inside the loop (one SSH round trip); terminal output waits for it, which is
acceptable at connection-open rates. `ponytail:` if a burst of accepts ever stalls rendering,
move the handle into its own owner task and have `ssh_main_task` hold only the channel.

**Local listener** (one task per spec, spawned before `ssh_main_task`):

```text
let listener = TcpListener::bind((spec.bind, spec.bind_port)).await;   // Err → warn notification, skip spec
loop { select! {
    _ = shutdown.cancelled() => break,
    Ok((tcp, peer)) = listener.accept() => tokio::spawn(relay_local(tcp, peer, target, open_tx.clone(), shutdown.clone())),
}}
// relay_local: send OpenDirectTcpip, await reply, copy_bidirectional(tcp, channel.into_stream()); errors logged at debug; the TCP side is closed on any failure
```

**Dynamic listener** = local listener whose `relay` first performs the SOCKS5 handshake
(RFC 1928) on `tcp`, no new dependency:

| Step | Bytes | Behaviour |
| --- | --- | --- |
| Greeting | `05 NMETHODS METHODS...` | reply `05 00` if `00` (no auth) offered, else `05 FF` and close |
| Request | `05 CMD 00 ATYP DST.ADDR DST.PORT` | `CMD` must be `01` CONNECT; `ATYP` `01` IPv4, `03` domain (len-prefixed), `04` IPv6 |
| Reply | `05 REP 00 01 00000000 0000` | `REP` `00` after the channel opened, `07` for BIND / UDP ASSOCIATE, `01` when the open failed; then close on any non-zero |

The destination host is passed to `channel_open_direct_tcpip` as text (domains resolve on the
remote side, which is the point of SOCKS through SSH). Malformed input, a greeting longer than
257 bytes, or a 5 s idle before the request closes the socket. SOCKS4/4a is not supported.

**Remote forward** (before `ssh_main_task`, the `open_sftp` precedent):

```text
match handle.tcpip_forward(spec.bind_host, spec.bind_port as u32).await {
    Ok(true) => forward_table.insert((spec.bind_host, spec.bind_port), (spec.target_host, spec.target_port)),
    _ => warn notification "Remote forward <bind>:<port> refused by the server", skip
}
```

`forward_table: Arc<Mutex<HashMap<(String, u32), (String, u16)>>>` is shared with the
handler (constructed before connect, so the handler can own an `Arc` clone). In
`server_channel_open_forwarded_tcpip(channel, connected_address, connected_port, ..)`:
look up `(connected_address, connected_port)`; when present, spawn
`copy_bidirectional(TcpStream::connect(target), channel.into_stream())` under the shutdown
token; when absent (server forwarding something OneTerm never asked for), drop the channel and
log at warn. `bind_port == 0` asks the server to pick; the port it returns is what the table
stores and what the info log prints.

**Teardown.** The session `CancellationToken` (today's `sftp_shutdown`, renamed
`session_shutdown`) is cancelled first in the existing teardown block; listeners break their
loops and drop (ports freed), relay tasks finish their copy or abort. `cancel_tcpip_forward` is
not sent: the transport disconnect that follows revokes every remote forward server-side.

**Notifications.** A skipped forward produces one warning through the session event sink
(same path as the keepalive disconnect notice); a fully failed spec list never blocks the
shell. Successful binds are logged at info with `PortForward::summary()`.

**Session UI.** `SshSession.port_forwards: Vec<PortForward>` (`serde(default,
skip_serializing_if = "Vec::is_empty")`). The dialog renders one row per spec (kind dropdown,
bind, port, target host, target port, remove) plus "Add"; `Dynamic` hides the target fields.
`into_session` runs `validate()` on every row and rejects duplicates of the same
`(kind, bind, bind_port)`. Duplicate Session copies the list; the duplicate's local binds fail
with the "address already in use" warning and the shell still opens (DEC-0011).

## Interfaces

```text
// oneterm-core
pub enum PortForward { .. }                       // see above
pub enum PortForwardError { Port, EmptyHost, Whitespace }
impl SshConfig { pub port_forwards: Vec<PortForward> }

// oneterm-ssh (crate-private)
pub(crate) enum HandleRequest { OpenDirectTcpip { .. } }
pub(crate) type ForwardTable = Arc<Mutex<HashMap<(String, u32), (String, u16)>>>;
pub(crate) async fn start_forwards(specs: &[PortForward], handle: &Handle<SshClientHandler>, open_tx: mpsc::Sender<HandleRequest>, table: ForwardTable, shutdown: CancellationToken, notify: &SessionEventSink) -> usize /* started */;
pub(crate) async fn socks5_connect_target(tcp: &mut TcpStream) -> Result<(String, u16), SocksError>;
pub(crate) fn spawn_forwarded_tcpip(channel: Channel<Msg>, target: (String, u16), shutdown: CancellationToken);
```

## Edge Cases and Failure Modes

- [ ] Local bind port in use (a previous or duplicated session): warning, spec skipped, session continues.
- [ ] Bind address is not loopback (user typed `0.0.0.0`): honoured as typed; the dialog shows a one-line caution under the row ("reachable from other machines"); no firewall prompt is issued by OneTerm.
- [ ] Remote target unreachable for a local forward: the direct-tcpip open fails, the accepted TCP socket is closed immediately (the browser sees a reset), logged at debug.
- [ ] Remote forward when sshd has `AllowTcpForwarding no`: `tcpip_forward` returns false, warning, skipped.
- [ ] Remote forward `bind_host = "0.0.0.0"` without `GatewayPorts yes`: sshd binds loopback anyway; nothing OneTerm can detect, documented in the dialog caution.
- [ ] SOCKS5 client offers only username/password auth: `05 FF`, closed.
- [ ] SOCKS5 UDP ASSOCIATE / BIND: reply `07`, closed.
- [ ] Session disconnects with relays active: token cancelled, every `copy_bidirectional` aborts, sockets closed; nothing lingers past the teardown block.
- [ ] Server opens a forwarded-tcpip channel for a port not in the table: dropped and logged.
- [ ] Request channel full (32 pending opens): `send().await` back-pressures the accept task; the listener keeps accepting, connections queue in the kernel backlog.

## Verification

- [ ] `cargo test -p oneterm-core`: `validate` rejects port 0, empty host, whitespace; serde round-trip for all three kinds; `summary` text.
- [ ] `cargo test -p oneterm-ssh`: in-process `russh` server that serves direct-tcpip by echoing; `Local` forward on an ephemeral port echoes bytes end to end; `Dynamic` forward answers the SOCKS5 greeting + CONNECT and echoes; a `05 02` (BIND) request gets `07`; `Remote` forward: the test server calls `channel_open_forwarded_tcpip` and the bytes reach a local `TcpListener` the test owns; an unknown forwarded port is dropped; cancelling the token frees the bound port within the test.
- [ ] `cargo test -p oneterm-session-ui`: round-trip of `port_forwards`; duplicate row rejected.
- [ ] Manual (Windows): `curl http://127.0.0.1:8080` through a local forward to a remote `python -m http.server 80`; `curl --socks5 127.0.0.1:1080 http://example.com`; remote forward reached from the server with `curl http://127.0.0.1:9000`; closing the tab frees 8080 and 1080 (`netstat -an`).
