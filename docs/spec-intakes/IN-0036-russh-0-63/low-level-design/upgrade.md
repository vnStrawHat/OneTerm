# Low-Level Design: every call site the russh 0.63 / russh-sftp 3.0 bump changes

Intake: IN-0036
HLD: [`../high-level-design.md`](../high-level-design.md)
Topic: upgrade
Date: 2026-09-15

> One concern per file. Keep this focused on implementation-level mechanics for a single area of the HLD so it stays reviewable. Do not restate the whole intake here.

## Concern

The exact set of OneTerm call sites that must change to compile and behave correctly against
`russh 0.63.3` and `russh-sftp 3.0.0`, with before/after signatures and the behaviour each site
must preserve.

The set is closed and was established by compiling the bumped workspace: **10 compile errors
across 6 files**, plus **1 silent semantic change** (`FileAttributes::default()`) that the
compiler cannot catch. Nothing else in `crates/ssh`, `crates/sftp-ui` or `crates/tools` names an
API that moved. `crates/sftp-ui` contains no `russh` or `russh_sftp` identifier at all.

## Design

### Change A — `check_server_key` takes a `PublicKeyOrCertificate` (1 site, **security**)

`crates/ssh/src/handler.rs`

Before (russh 0.61.2):

```rust
async fn check_server_key(
    &mut self,
    server_key: &russh::keys::PublicKey,
) -> Result<bool, Self::Error> {
    let handler = self.clone();
    let server_key = server_key.clone();
    tokio::task::spawn_blocking(move || handler.verify_server_key(&server_key))
        .await
        .unwrap_or_else(|join_error| { /* KeyStore(IO(join_error)) */ })
}
```

After (russh 0.63.3):

```rust
async fn check_server_key(
    &mut self,
    server_key: &russh::keys::PublicKeyOrCertificate,
) -> Result<bool, Self::Error> {
    // russh 0.63 can be asked to accept an OpenSSH host certificate instead of a
    // bare key. OneTerm never advertises a `*-cert-v01@openssh.com` host-key
    // algorithm (`Preferred::host_key_certificates` is empty in
    // `Preferred::DEFAULT`, and OneTerm builds no custom `Preferred`), so a
    // conforming server never presents one. Refuse rather than guess: there is
    // no certificate-authority trust store to check it against, and
    // known_hosts records bare keys.
    let server_key = match server_key {
        russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
        russh::keys::PublicKeyOrCertificate::Certificate(certificate) => {
            return Err(SshHandlerError::UnknownHostKey {
                host: self.host.clone(),
                port: self.port,
                algorithm: certificate.algorithm().to_string(),
                fingerprint: format!("cert:{}", certificate.fingerprint(HashAlg::Sha256)),
            });
        }
    };
    let handler = self.clone();
    tokio::task::spawn_blocking(move || handler.verify_server_key(&server_key))
        .await
        .unwrap_or_else(|join_error| { /* unchanged */ })
}
```

Invariants this must preserve, all of them already asserted by `handler_tests.rs`:

- a key byte-equal to a known_hosts entry -> `Ok(true)`;
- a different key of the same algorithm -> `Err(ChangedHostKey)`;
- a key of an algorithm not in known_hosts while other entries exist ->
  `Err(HostKeyAlgorithmMismatch)`;
- no entry at all + `HostKeyPolicy::AcceptNewFingerprint(f)` matching the SHA-256 fingerprint ->
  learn, then `Ok(true)`;
- no entry at all + `Strict` (or a non-matching `AcceptNewFingerprint`) -> `Err(UnknownHostKey)`.

`verify_server_key`, `known_host_keys`, `learn_known_host`, `fingerprint` and
`preferred_key_algorithms` are **not touched**. The `hash_alg` field of the `PublicKey` arm is
deliberately discarded: it names the *signature* algorithm used for the exchange
(`rsa-sha2-256` vs `ssh-rsa`), not the key, and known_hosts records the bare key. Discarding it
keeps `verify_server_key`'s RSA matching identical to 0.61's.

### Change B — server-initiated channel opens carry a `ChannelOpenHandle` (2 client sites, **security**)

`crates/ssh/src/handler.rs`

In russh 0.61.2, `client/encrypted.rs` sent `CHANNEL_OPEN_CONFIRMATION` *before* calling the
handler. In 0.63.3 the handler receives a `russh::client::ChannelOpenHandle` and owns the reply:
`accept()`, `reject(ChannelOpenFailure)`, or drop (which sends
`ChannelOpenFailure::AdministrativelyProhibited`).

#### B1 `server_channel_open_agent_forward`

```rust
// before
async fn server_channel_open_agent_forward(
    &mut self,
    channel: russh::Channel<client::Msg>,
    _session: &mut client::Session,
) -> Result<(), Self::Error>

// after
async fn server_channel_open_agent_forward(
    &mut self,
    channel: russh::Channel<client::Msg>,
    reply: russh::client::ChannelOpenHandle,
    _session: &mut client::Session,
) -> Result<(), Self::Error>
```

Body: in the `Some((connector, shutdown))` arm, `reply.accept().await;` **before**
`spawn_agent_bridge(..)` — the bridge writes to the channel, and the peer must have the
confirmation first. In the `None` arm (forwarding off for this session), drop `reply` instead of
the old `channel.close().await`: the peer now gets `AdministrativelyProhibited`, where 0.61
confirmed the channel and then closed it. Strictly stronger, and the same outcome for the
user. The `log::warn!` line stays; the `report_best_effort("close unrequested agent channel", …)`
call and its `channel.close()` go away because there is no confirmed channel to close.

#### B2 `server_channel_open_forwarded_tcpip`

```rust
// before: (&mut self, channel, connected_address, connected_port,
//          originator_address, originator_port, &mut client::Session)
// after:  (&mut self, channel, connected_address, connected_port,
//          originator_address, originator_port, reply: ChannelOpenHandle,
//          &mut client::Session)
```

Body: in the `(Some(target), Some((_, shutdown)))` arm, `reply.accept().await;` before
`spawn_forwarded_tcpip(..)`. In the `_` arm (a forwarded-tcpip channel for a listener OneTerm
never requested), drop `reply` instead of closing. This is the DEC-0011 rule
("channels for listeners OneTerm never asked for are dropped, never bridged") expressed on the
wire for the first time.

### Change C — in-process test/dev servers: `Result<bool, E>` -> `Result<(), E>` + handle (6 sites)

The server-side `Handler::channel_open_*` methods changed the same way, and their `bool` return
(which used to mean accept/reject) is gone. russh's **default** implementation is
`async { Ok(()) }`, which drops the handle and therefore **rejects** — so every accepting server
must now say so explicitly.

| File | Method | Before -> After |
|---|---|---|
| `crates/ssh/src/agent_tests.rs:222` (`AgentForwardServer`) | `channel_open_session` | `-> Result<bool, E> { Ok(true) }` -> `reply: ChannelOpenHandle` param, `-> Result<(), E> { reply.accept().await; Ok(()) }` |
| `crates/ssh/src/route_tests.rs:34` (`RelayServer`) | `channel_open_session` | same |
| `crates/ssh/src/route_tests.rs:42` (`RelayServer`) | `channel_open_direct_tcpip` | `if !self.relay { return Ok(false) }` -> `reply.reject(ChannelOpenFailure::AdministrativelyProhibited).await; return Ok(());` — this is the "jump host refuses to relay" case and **must stay a rejection**, not a silent success. The success path gains `reply.accept().await` before `channel.into_stream()`. |
| `crates/ssh/src/session.rs:994` (`EnvRecordingServer`) | `channel_open_session` | as row 1 |
| `crates/ssh/src/task_tests.rs:51` (test server) | `channel_open_session` | as row 1 |
| `crates/ssh/src/tunnel_tests.rs:48` (`EchoServer`) | `channel_open_direct_tcpip` | `Ok(true)` after spawning the echo task -> `reply.accept().await;` before spawning, then `Ok(())` |
| `crates/tools/src/bin/sftp-dev-server.rs:67` (`ClientHandler`) | `channel_open_session` | `self.channels.insert(channel.id(), channel); Ok(true)` -> `reply.accept().await;` then insert, then `Ok(())` |

`accept()` goes **before** any use of the channel in every case, so no data is written to an
unconfirmed channel.

`tcpip_forward`, `agent_request`, `shell_request`, `pty_request`, `env_request`,
`auth_password`, `auth_publickey` and `auth_keyboard_interactive` are **unchanged** in 0.63 and
are not touched.

### Change D — `FileAttributes::default()` flipped meaning (1 site, compiler-silent)

`crates/tools/src/bin/sftp-dev-server.rs:320`, in `opendir`'s per-entry metadata fallback:

```rust
// before (russh-sftp 2.3: Default == dummy)
Err(_) => FileAttributes::default(),
// after (russh-sftp 3.0: Default == empty; the old value is now `dummy()`)
Err(_) => FileAttributes::dummy(),
```

Without this, an entry whose `std::fs::metadata` call fails would be sent to the client with
every attribute omitted instead of the previous placeholder directory attributes, and the
browser would render it with no size, no permissions and no mtime. This is the only site in the
workspace that relied on `FileAttributes::default()`; `crates/ssh/src/sftp_task/transfer.rs:105`
uses `FileAttributes::empty()`, whose meaning did not change.

### Change E — new coverage for key-file parsing (new file)

The bump moves `ssh-key` `rc.10 -> rc.11` and takes `ecdsa`, `ed25519-dalek`, `p256/384/521`,
`ssh-cipher` and `ssh-encoding` from release candidate to stable. `load_secret_key`
(`crates/ssh/src/session.rs:28`) decodes every private key OneTerm reads from disk through that
stack, and **had no direct test** — the loopback suites all generate keys in memory.

New `crates/ssh/src/keyfile_tests.rs`, one focused module:

- for each of Ed25519, ECDSA P-256, ECDSA P-384, ECDSA P-521 and RSA-2048: generate a
  `PrivateKey`, write it in OpenSSH format to a temp file, `load_secret_key(path, None)`, assert
  the loaded public key equals the generated one;
- for Ed25519 and RSA-2048: write the same key **encrypted** with a passphrase, assert
  `load_secret_key(path, Some(passphrase))` round-trips, and assert a wrong passphrase is an
  `Err` (not a panic and not a silent success).

This is the one gap the change opens that the existing suites do not close; everything else the
bump touches already has loopback coverage.

## Interfaces

Root `Cargo.toml`, `[workspace.dependencies]`:

```toml
# before
russh = { version = "0.61", default-features = false, features = ["ring", "flate2", "rsa"] }
russh-sftp = "2.3"
# after
russh = { version = "0.63", default-features = false, features = ["ring", "flate2", "rsa"] }
russh-sftp = "3.0"
```

Feature set unchanged (`ring`, `flate2`, `rsa`; `default-features = false`). No crate is added
to or removed from `[workspace.dependencies]`; `russh-cryptovec` and `russh-util` stay
transitive, as `docs/agents/dependencies.md` §3 requires.

New russh types referenced by OneTerm after the change:

```rust
russh::keys::PublicKeyOrCertificate   // re-export of russh::cert::PublicKeyOrCertificate
russh::client::ChannelOpenHandle      // = ChannelOpenHandleInner<client::Msg>
russh::server::ChannelOpenHandle      // = ChannelOpenHandleInner<server::Msg>
russh::ChannelOpenFailure             // AdministrativelyProhibited used in route_tests
russh_sftp::protocol::FileAttributes::dummy()
```

No OneTerm public API changes: `SshSession`, `SshConnectParams`, `SshHandlerError`,
`SftpTask`, `AppError` and `SftpStatus` keep their shapes, so `oneterm-sftp-ui`,
`oneterm-terminal` and `oneterm-session-ui` need no edit.

## Edge Cases and Failure Modes

- [ ] **Server presents a host certificate.** Cannot happen with `Config::default()` (OneTerm
      advertises no certificate algorithm), but if a non-conforming server sends one anyway,
      `check_server_key` returns `Err(UnknownHostKey { fingerprint: "cert:…" })`, russh turns a
      `false`/`Err` into `Error::UnknownKey`, and the connect fails closed with a message the
      user can act on. **Never `Ok(true)`.**
- [ ] **Agent channel arrives while forwarding is off.** `reply` is dropped ->
      `AdministrativelyProhibited` on the wire. Previously: confirm, then close. The session
      continues either way; only the peer-visible reason improves.
- [ ] **forwarded-tcpip for an unrequested listener.** Same: dropped handle, explicit refusal.
- [ ] **Jump host that refuses to relay** (`route_tests::RelayServer { relay: false }`). Must
      stay an explicit `reject`, so `route.rs`'s error path is still exercised; an accidental
      `accept()` there would turn a red test green for the wrong reason.
- [ ] **Encrypted key with the wrong passphrase.** `load_secret_key` must return `Err`, not
      panic — asserted by the new `keyfile_tests`.
- [ ] **`accept()` called after writing to the channel.** Would race the confirmation against
      data. Avoided by ordering `accept()` first at every site.
- [ ] **SFTP server with a small window and 16 in-flight writes.** russh-sftp respects the
      server-advertised limits; the change is a client-side budget, not a protocol change. Not
      exercised against a real server — recorded as a gap in `US-0095`.

## Verification

- [ ] `cargo test -p oneterm-ssh` — the host-key suite (`handler_tests.rs`: known-key accept,
      changed-key reject, algorithm-mismatch reject, learn-on-accept, known_hosts round trip for
      ed25519 / ECDSA-P256 / RSA, and a loopback `client::connect` that drives the real
      `check_server_key`) covers Change A.
- [ ] `cargo test -p oneterm-ssh` — `tunnel_tests.rs` (direct-tcpip echo, forwarded-tcpip
      delivery, unrequested-forward drop), `agent_tests.rs` (agent auth over a real in-process
      `russh::keys::agent::server::serve`, agent-forward accept and refuse), `route_tests.rs`
      (two-hop jump-host relay and the relay-refused error) cover Changes B and C.
- [ ] `cargo test -p oneterm-ssh keyfile` — Change E.
- [ ] `cargo test -p oneterm-tools` — covers the `sftp-dev-server` compile and its own suite;
      Change D's runtime effect is exercised by running the dev server and listing a directory.
- [ ] `cargo tree` — zero `russh 0.61`, zero `russh-sftp 2.3`, `-rc` count 16 -> 3.
- [ ] `cargo deny check licenses bans advisories`, `python scripts/third-party-notices.py --check`,
      `python scripts/verify-dependency-graph.py`, `pwsh scripts/ci-local.ps1 --full`.
