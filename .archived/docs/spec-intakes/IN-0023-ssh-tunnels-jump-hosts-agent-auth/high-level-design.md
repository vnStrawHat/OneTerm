# High-Level Design: SSH tunnels, jump hosts, and agent auth

Intake: IN-0023
Lane: high_risk
Date: 2026-09-10

## Idea

`SshConfig` grows from "one host, one credential" into a **connection route**: an ordered list
of jump hops followed by the target, a list of port forwards, and an agent-forwarding switch.
`crates/ssh` walks the route hop by hop — the first hop over TCP, every later hop over a
direct-tcpip channel of the previous hop turned into a stream for `russh::client::connect_stream`
— and authenticates each hop with its own `SshAuthMethod`, which gains an `Agent` variant
backed by the local SSH agent. Once the target is authenticated, the session channel is opened
as today; before the PTY request, agent forwarding is requested when enabled and the port
forward listeners are started. All listener and relay tasks die with the session through the
`CancellationToken` that already kills the SFTP task.

The saved-session model stores only references and non-secret specs: `jump_host` is the id
of another saved session, `port_forwards` is a list of `PortForward` specs, `agent_forwarding`
is a bool, and `auth_method` may be `agent`. Credentials for every hop are typed at connect
time and live only in RAM (DEC 0001, DEC 0002 applied per hop).

## Diagram

```text
 oneterm-session-ui                       oneterm-ssh (hidden tokio runtime)
 +----------------------------+           +-----------------------------------------------------+
 | SshSession (ssh_session.json)         | connect(cfg)                                          |
 |  auth_method: agent        | SshConfig |   hop[0]  -- TCP ---------------> bastion             |
 |  jump_host: Some(id)       |---------->|   hop[1]  -- direct-tcpip(bastion) --> relay          |
 |  port_forwards: [...]      | Session   |   target  -- direct-tcpip(relay) -> connect_stream    |
 |  agent_forwarding: true    | Factory   |   authenticate(hop): none | password | key | agent    |
 +----------------------------+           |   channel_open_session -> agent_forward? -> pty/shell |
        ^ credentials per hop             |   forwards: Local TcpListener / Dynamic SOCKS5        |
        | (connect dialog)                |             Remote tcpip_forward + handler callback   |
        |                                 |   ssh_main_task { handle, jump_handles, open_rx }     |
 Local SSH agent <------------------------+   agent.rs: named pipe | Pageant | SSH_AUTH_SOCK      |
 (signer for auth, target of forwarding)  +-----------------------------------------------------+
```

`Handle` is not `Clone` and is moved into `ssh_main_task`; a `HandleRequest` mpsc lets listener
tasks ask that task to open a direct-tcpip channel for each accepted connection.

## UI Wireframe

Session dialog (add / edit), new rows marked with `*`:

```text
+----------------------------------------------------------------------+
| Session                                                          [x] |
+----------------------------------------------------------------------+
| Label     [ prod-db                    ]   Group  [ prod          v ] |
| Host      [ 10.0.5.20                  ]   Port   [ 22              ] |
| Username  [ deploy                     ]   Color  [ # ]               |
| Authentication   ( ) Password   ( ) Private key   (o) SSH agent    * |
|   (password / key path + passphrase rows as today; none for agent)   |
| Jump host        [ bastion  (ops@bastion.example.com:22)        v ] * |
|                  "None" by default; the list excludes this session   |
| [x] Forward the SSH agent to the remote host                       * |
| Port forwards                                            [+ Add]   * |
|  [Local   v] [127.0.0.1] [ 8080] -> [localhost   ] [   80]     [x]   |
|  [Remote  v] [127.0.0.1] [ 9000] -> [127.0.0.1   ] [ 3000]     [x]   |
|  [Dynamic v] [127.0.0.1] [ 1080]    (SOCKS5)                   [x]   |
| Logging   (o) Inherit  ( ) On  ( ) Off                               |
+----------------------------------------------------------------------+
|                                               [ Cancel ]  [ Save ]   |
+----------------------------------------------------------------------+
```

Quick Connect gains only the SSH agent radio and the jump host combobox (owner decision,
2026-09-10). Port forwards and agent forwarding are saved-session features.

Connect dialog for a session with one jump host (jump host uses an encrypted key, target uses
a password; an agent hop shows a single "SSH agent — nothing to enter" line):

```text
+------------------------------------------------------+
| Connect to prod-db                               [x] |
+------------------------------------------------------+
| Jump host  bastion  (ops@bastion.example.com:22)     |
|   Private key   ~/.ssh/id_ed25519                    |
|   Passphrase    [ ************           ]           |
| ---------------------------------------------------- |
| Target     deploy@10.0.5.20:22                       |
|   Password      [ ************           ]           |
+------------------------------------------------------+
|                              [ Cancel ]  [ Connect ] |
+------------------------------------------------------+
```

Host-key confirmation names the hop:

```text
The server is not present in your OpenSSH known_hosts file.

Host: bastion.example.com:22  (jump host for prod-db)
Algorithm: ssh-ed25519
SHA-256 fingerprint: SHA256:...
```

Notifications (toast, existing component):

```text
[warn] Port forward 127.0.0.1:8080 not started: address already in use. The session stays connected.
[warn] The server refused agent forwarding for prod-db.
```

## Data Flow

1. **Save.** The session dialog validates the new rows (`PortForward::validate`: port 1..65535,
   bind address parses as an IP, target host non-empty; jump host is not this session) and
   writes `SshSession { auth_method, jump_host, port_forwards, agent_forwarding, .. }` to
   `ssh_session.json` through the existing store (schema v2, additive fields, serde defaults).
2. **Connect (UI).** `connect_ssh_session` resolves the jump chain from the store
   (`resolve_jump_chain`: follow `jump_host` ids, error on a missing id, a cycle, or more than
   `MAX_JUMP_HOPS` = 4 hops), opens the connect dialog with one credential block per hop plus
   the target, and builds `SshConfig { jump_hops: Vec<SshHop>, auth, port_forwards,
   agent_forwarding, .. }`. Secrets are cleared from the fields after submission as today.
3. **Route (backend).** `connect` walks `jump_hops` then the target. For hop 0 it calls
   `client::connect(cfg, addr, handler)`; for every later hop and for the target it calls
   `prev_handle.channel_open_direct_tcpip(host, port, "127.0.0.1", 0)`, turns the channel into
   a stream, and calls `client::connect_stream(cfg, stream, handler)`. Each hop gets its own
   `SshClientHandler` with its own `HostKeyPolicy`, so `known_hosts` checks and the unknown-host
   error carry that hop's host and port. Each hop runs under `ConnectPhase::Transport` then
   `ConnectPhase::Authentication`; error messages are prefixed with `jump host user@host:port:`.
4. **Authenticate.** The inline `match cfg.auth` of today becomes `authenticate(handle, user,
   auth)` and gains `SshAuthMethod::Agent`: connect to the local agent, list identities, try
   each with `authenticate_publickey_with(user, pubkey, hash_alg, &mut agent)` until one
   succeeds (RSA keys pick the hash exactly as the file-key path does), stop when the server
   drops `publickey` from `remaining_methods`, and report "none of the N agent identities was
   accepted" otherwise.
5. **Session channel.** As today: `channel_open_session`. When `agent_forwarding` is set,
   `channel.agent_forward(true)`; a refusal becomes a warning notification, not a failure. Then
   the PTY and shell requests, unchanged.
6. **Forwards.** Before `ssh_main_task` is spawned (the `open_sftp` precedent): `Local` and
   `Dynamic` specs bind a `TcpListener` and spawn an accept loop; `Remote` specs call
   `handle.tcpip_forward(bind_host, bind_port)` and register `(bind_host, port) -> target` in
   the handler's forward table. A bind or `tcpip_forward` failure emits one warning and is
   skipped; the session continues (DEC-0011).
7. **Relay.** A local accept sends `HandleRequest::OpenDirectTcpip { host, port, reply }` to
   `ssh_main_task`, which owns the handle; the reply carries the `Channel`, and the accept task
   spawns `copy_bidirectional(tcp, channel.into_stream())`. Dynamic listeners first run the
   SOCKS5 greeting + CONNECT parse, then continue like a local forward. Remote connections
   arrive in `SshClientHandler::server_channel_open_forwarded_tcpip`, which looks up the
   target and spawns `copy_bidirectional(TcpStream::connect(target), channel.into_stream())`.
   Agent-forward channels arrive in `server_channel_open_agent_forward` and are bridged to a
   fresh local agent connection, or closed immediately when forwarding is not enabled.
8. **Teardown.** The existing single teardown block cancels the session token (listeners,
   relays, SFTP), closes the target channel, disconnects the target handle, then drops the
   jump handles in reverse order so no hop outlives the hop that carries it.

## Crate placement

- `crates/core/src/ssh_config.rs`: `SshAuthMethod::Agent`, `SshHop`, `SshConfig.{jump_hops,
  port_forwards, agent_forwarding}`, `MAX_JUMP_HOPS`.
- `crates/core/src/port_forward.rs` (new): `PortForward`, `ForwardKind`, `validate`, serde
  (persisted by `session-ui`, consumed by `ssh`; `core` is the lowest crate both see, R10).
- `crates/ssh/src/agent.rs` (new): agent discovery, `AgentStream`, identity loop, forwarding
  bridge.
- `crates/ssh/src/route.rs` (new): `open_transport` (TCP or direct-tcpip + `connect_stream`),
  `authenticate`, `JumpHandles`, hop error context.
- `crates/ssh/src/tunnel.rs` (new): listeners, SOCKS5 CONNECT parser, relay tasks,
  `HandleRequest`, forward table.
- `crates/ssh/src/handler.rs`: `agent_forwarding` flag, forward table,
  `server_channel_open_forwarded_tcpip`, `server_channel_open_agent_forward`.
- `crates/ssh/src/task.rs`: `open_rx` select arm, `_jump_handles`.
- `crates/session-ui`: `SshAuthPreference::Agent`, `SshSession.{jump_host, port_forwards,
  agent_forwarding}`, `resolve_jump_chain`, jump-host combobox (reuse the `group_combo`
  pattern), forward list rows, per-hop `SshAuthForm` blocks in the connect dialog, hop-aware
  host-key confirmation.
- No UI crate imports `oneterm-ssh` (R3); no new third-party dependency (`russh` 0.61 ships
  the agent client and `pageant` on Windows; `tokio` already has `net`).

## Detail Design

- [x] Detail design: required (high-risk)
- Reason: three concerns are pinned before code — `low-level-design/agent-auth.md` (agent
  discovery, identity loop, forwarding bridge), `low-level-design/jump-hosts.md` (route,
  handle lifetime, per-hop host keys and credentials), `low-level-design/port-forwarding.md`
  (listeners, SOCKS5 subset, remote callback, teardown).
