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
            return Err(SshHandlerError::HostCertificate {
                host: self.host.clone(),
                port: self.port,
                algorithm: certificate.algorithm().to_string(),
                // The fingerprint of the key *inside* the certificate: the value
                // an operator can compare against a known_hosts line.
                // `Certificate` has no `fingerprint()` of its own.
                fingerprint: format!(
                    "cert:{}",
                    certificate.public_key().fingerprint(HashAlg::Sha256)
                ),
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

`SshHandlerError::HostCertificate` is a **new variant**, not a reuse of `UnknownHostKey`, and
that distinction is the whole of the fix. `SshHandlerError::to_app_error` maps `UnknownHostKey`
to `AppError::HostKeyUnknown`, which `crates/session-ui/src/common.rs` answers with the first-use
"trust this host key?" dialog. Approving it retries with
`HostKeyPolicy::AcceptNewFingerprint("cert:…")`, and the certificate arm returns `Err`
unconditionally *before* any policy check — so there was never a trust bypass, but the user was
shown an Accept button that can only fail again. `HostCertificate` maps to
`AppError::Connect { phase: Transport, .. }` instead: an ordinary connect failure whose message
names host certificates as unsupported and gives the certified key's fingerprint. No new
`AppError` variant and no UI change are needed.

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

**Scope of "now refuses" — informational.** The client trait has seven hooks that can receive a
server-initiated channel open; OneTerm overrides **two** of them
(`server_channel_open_agent_forward`, `server_channel_open_forwarded_tcpip`). In russh 0.63.3 the
**client-side** defaults for the other five all *accept*
(`russh-0.63.3/src/client/mod.rs` ~2477-2600: `server_channel_open_session`,
`server_channel_open_x11`, `server_channel_open_direct_tcpip`,
`server_channel_open_direct_streamlocal`, `server_channel_open_forwarded_streamlocal` each run
`reply.accept().await`; only `server_channel_open_unknown` drops the handle, gated behind
`should_accept_unknown_server_channel`, default `false`). So a server-initiated session, x11,
direct-tcpip or streamlocal channel is still confirmed and then dropped — exactly what 0.61 did.
**This is not a regression** and nothing OneTerm asked for changed; the statement "unrequested
channels are now refused" applies specifically to agent-forward and forwarded-tcpip. Closing the
remaining five would be five one-line `reply.reject(…)` overrides, and is deliberately out of
scope here: it is a fail-closed posture change to argue on its own evidence, not a consequence of
the bump.

Note that Change C's "russh's default drops the handle and therefore rejects" is correct for the
**server** trait, which is what Change C is about (`russh-0.63.3/src/server/mod.rs:362,377,395,417`
are all `async { Ok(()) }`). It does not hold on the client side.

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

### Change F — pin the SFTP transfer budget back to 2.3.0's (1 site, **throughput**)

Added at acceptance rework. russh-sftp 3.0 introduced a *third* size cap that 2.3.0 did not have,
and the first pass missed it because it only compared `max_concurrent_writes` (8 -> 16).

`russh-sftp-3.0.0/src/client/fs/file.rs:374-395`, `poll_write`:

```rust
let packet_write_len    = features.max_packet_len       - (25 + handle.len());  // 256 KiB default
let server_write_len    = features.limits.and_then(|l| l.write_len).unwrap_or(u32::MAX as u64);
let preferred_write_len = features.max_write_packet_len - (25 + handle.len());  //  32 KiB default  <-- NEW
let len = buf.len().min(packet_write_len).min(server_write_len).min(preferred_write_len);
```

2.3.0 had only the first two terms. OneTerm feeds `CHUNK_LEN = 255 KiB`, so the new third term is
always the binding one, and SFTP throughput — `in-flight bytes / RTT` — collapses:

| | packet payload | in flight | in-flight bytes |
|---|---|---:|---:|
| russh-sftp 2.3.0 | 261 120 B (one per chunk) | 8 | **2 088 960** |
| russh-sftp 3.0.0 defaults | 32 742 B (8 per chunk) | 16 | **523 872** (4.0x less) |
| after this change | **261 120 B measured** | 8 | **2 088 960** (2.3.0 restored exactly) |

`crates/ssh/src/session.rs`, `open_sftp`:

```rust
// before
let sftp_channel = russh_sftp::client::SftpSession::new(stream).await?;
// after
let sftp_channel =
    russh_sftp::client::SftpSession::new_with_config(stream, sftp_config()).await?;
```

with one named constructor next to it so the production path and the regression test cannot
drift:

```rust
/// russh-sftp client config pinned to the transfer budget russh-sftp 2.3.0 had.
pub(crate) fn sftp_config() -> russh_sftp::client::Config {
    russh_sftp::client::Config {
        // 3.0's 32 KiB default caps every SSH_FXP_WRITE and cuts the in-flight
        // write budget 4x. Raise it to `max_packet_len` so the packet size is
        // bounded only by the SFTP packet limit and the server's own
        // `limits@openssh.com` reply, exactly as 2.3.0 was.
        max_write_packet_len: 262_144,
        // 2.3.0's value. With 255 KiB packets this is ~2 MiB in flight; 3.0's
        // 16 would double it, which is an unreviewed change, not a bump.
        max_concurrent_writes: 8,
        // OneTerm stripes its own downloads (`transfer::pipeline::copy_striped`
        // seeks per chunk), which discards 3.0's read-ahead after the requests
        // are already on the wire. One in flight per handle; the striping
        // supplies the concurrency. See Change G.
        max_concurrent_reads: 1,
        ..Default::default()
    }
}
```

The server's advertised `limits@openssh.com` `max-write-length` needs no code: `server_write_len`
above already clamps to it whenever the server sends the extension, and `SftpSession::new_with_config`
already clamps `max_packet_len` to the server's `packet_len`
(`russh-sftp-3.0.0/src/client/session.rs:74`). Raising `max_write_packet_len` only removes
russh-sftp's *own* extra cap; it can never exceed what the server allows.

One pre-existing edge, neither caused nor changed by this: a server that does **not** advertise
`limits@openssh.com` leaves `server_write_len` at `u32::MAX`, so the only cap is
`max_packet_len - 25 - handle.len()` ~ 262 KiB. russh-sftp 2.3.0 behaved identically, so this is
the behaviour OneTerm has shipped all along, not something the bump introduces.

### Change G — stop paying for read-ahead OneTerm throws away (same site)

`transfer::pipeline::read_chunk` seeks before **every** chunk. russh-sftp 3.0 answers a read by
putting `max_concurrent_reads` (16) `SSH_FXP_READ` packets on the wire immediately
(`fs/file.rs ReadState::request` -> `rawsession.rs:451 read_nowait` -> `send`), and `poll_seek`
calls `ReadState::reset`, which clears only the **local** queue. The server has already served
the discarded requests.

Two candidate fixes, both measured on the same 5 MiB loopback download (see the packet's PROOF):

| | bytes on the wire | READ requests | wall time |
|---|---:|---:|---:|
| striping + `max_concurrent_reads: 16` (shipped a33a994) | 48 700 595 B (**9.29x**) | 201 | 61.1 ms |
| striping + `max_concurrent_reads: 1` (**chosen**) | 5 263 100 B (1.00x) | 21 | 8.4 ms |
| no striping + `max_concurrent_reads: 16` | 5 242 880 B (1.00x) | 21 | 8.1 ms |

All three rows are the same 5 MiB file, so the control is **9.29x**, not the 3.6x the
independent verification measured — that run used a 2 MiB file, and the amplification grows with
file size. The chosen row's 20 220 B over the file is arithmetic, not read-ahead: russh-sftp asks
for `max_packet_len - READ_OVERHEAD_LENGTH` = 262 131 B per request while OneTerm consumes
`CHUNK_LEN` = 261 120 B, so each seek drops 1 011 B and 20 x 1 011 = 20 220 exactly. russh-sftp
2.3.0 read the same way.

Both candidates remove the amplification; the measured winner is recorded in the packet and the
choice is `max_concurrent_reads: 1`. Dropping the striping instead would delete `copy_striped`,
`read_handles_for`, `REORDER_WINDOW` and their tests, and re-open resume, progress and
cancellation behaviour that this dependency-bump packet has no mandate to change — a larger diff
for the same measured result.

> **Closed by `IN-0037` (2026-09-15).** The owner ordered the other candidate. The striping,
> `read_handles_for` and `REORDER_WINDOW` are deleted and `max_concurrent_reads` is back at
> russh-sftp 3.0's 16, which is what this table's third row measured. Both rows still serve the
> file exactly once; row 3 wins on the in-flight budget (4.2 MB against 1.04 MB), which is what
> throughput is made of on a link with RTT. Progress, cancellation and the shrinking-file clamp
> are each preserved under a test — see
> [`../../IN-0037-sftp-library-read-ahead/US-0096-library-read-ahead.md`](../../IN-0037-sftp-library-read-ahead/US-0096-library-read-ahead.md).
> The write budget (Change F) is untouched by that change.

### Change E — new coverage for key-file parsing (new file)

The bump moves `ssh-key` `rc.10 -> rc.11` and takes `ecdsa`, `ed25519-dalek`, `p256/384/521`,
`ssh-cipher` and `ssh-encoding` from release candidate to stable. `load_secret_key`
(`crates/ssh/src/session.rs:28`) decodes every private key OneTerm reads from disk through that
stack, and **had no direct test** — the loopback suites all generate keys in memory.

New `crates/ssh/src/keyfile_tests.rs`, one focused module (5 tests):

- **Generated** keys, round-tripped through a file: Ed25519, ECDSA P-256, P-384, P-521. RSA is
  **not** generated — RSA key generation is far too slow for a debug-build unit test, the same
  reason `handler_tests.rs` keeps a fixed RSA public key.
- **RSA** is a fixed 2048-bit fixture written by `ssh-keygen`, loaded unencrypted and re-encoded.
  Using real OpenSSH output makes this the one case that proves interoperability with OpenSSH's
  writer rather than a russh round trip.
- **Encrypted**: Ed25519 only — loads with the right passphrase, errors on the wrong one, errors
  with no passphrase.

Encrypted **RSA** is covered by Change H's adopted suite
(`us0095_verify_tests::an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase`), which encrypts
the same fixture rather than generating a key.

This is the one gap the change opens that the existing suites do not close; everything else the
bump touches already has loopback coverage.

### Change H — adopt the independent verification's 11 tests (new file)

Added at acceptance rework. `crates/ssh/src/us0095_verify_tests.rs`, wired from
`crates/ssh/src/lib.rs`, taken from the verification recorded in
[`../evidence/US-0095-verify.md`](../evidence/US-0095-verify.md). It closes the gap this LLD
listed as untestable and adds the key-file cases Change E omits:

- `a_recorded_host_key_is_accepted_through_the_handshake`,
  `an_unrecorded_host_key_is_unknown_through_the_handshake`,
  `a_changed_host_key_is_refused_as_changed_not_unknown` — the `PublicKey` arm end to end
  through a real loopback handshake, not just `verify_server_key` in isolation;
- `a_host_certificate_is_refused_even_when_its_inner_key_is_trusted` — builds a real self-signed
  host certificate, serves it from a loopback `russh::server` with `Config::certificates`,
  forces negotiation by setting `preferred.host_key_certificates` on the **client**, and
  pre-records the certificate's inner key in known_hosts first. A fall-through to the inner key
  would make this connect succeed. It does not. **This closes the gap Change A could only argue
  from code reading.** Adopted with one change: it now expects `SshHandlerError::HostCertificate`
  rather than `UnknownHostKey`, and additionally asserts `to_app_error()` yields
  `AppError::Connect` — the Change A fix above;
- `an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase`, `a_pkcs8_pem_ed25519_key_loads`,
  `a_key_file_with_crlf_line_endings_records_its_outcome` (it loads: `str::lines()` strips the
  `\r`), `an_empty_passphrase_on_an_encrypted_key_is_refused`,
  `a_truncated_key_file_errors_rather_than_panics`;
- `seek_per_chunk_reads_measure_the_new_pipeline_read_ahead` and
  `a_five_mib_round_trip_matches_and_records_the_write_packet_budget` — the measurements behind
  Changes F and G, and a 5 MiB byte-compared SFTP round trip in both directions.

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
