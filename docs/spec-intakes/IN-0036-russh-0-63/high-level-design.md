# High-Level Design: russh 0.61 -> 0.63, russh-sftp 2.3 -> 3.0

Intake: IN-0036
Lane: high-risk
Date: 2026-09-15

## Idea

Move the two workspace requirements `russh = "0.61"` and `russh-sftp = "2.3"` to `"0.63"` and
`"3.0"`. Everything else in the crypto stack follows from `Cargo.lock` re-resolution, because
nothing below `russh` is a workspace dependency.

The point of the bump is not a feature: it is to leave the RustCrypto **release candidates** that
`russh 0.61.2` pins. `russh 0.63` requires the released RustCrypto line wherever one exists, so
the bump converts 13 of the 16 `-rc` crates in `Cargo.lock` to stable in one step.

Two API changes fall out of it, both on the security-relevant surface, and both are the reason
this intake is high-risk rather than routine:

1. `client::Handler::check_server_key` now takes `&PublicKeyOrCertificate` — russh 0.63 gained
   OpenSSH host-certificate support.
2. Server-initiated channel opens are no longer auto-confirmed by russh before the handler runs.
   The handler is handed a `ChannelOpenHandle` and owns accept/reject.

## Versions crossed

### Workspace requirements (root `Cargo.toml`)

| Requirement | Before | After |
|---|---|---|
| `russh` | `0.61`, `default-features = false`, features `ring`, `flate2`, `rsa` | `0.63`, same features |
| `russh-sftp` | `2.3` | `3.0` |

### Resolved graph (`Cargo.lock`)

Crates that move from a release candidate to a stable release — the whole point of the change:

| Crate | Before | After |
|---|---|---|
| `aes-gcm` | `0.11.0-rc.4` | `0.11.1` |
| `argon2` | `0.6.0-rc.8` | `0.6.0` |
| `blake2` | `0.11.0-rc.6` | `0.11.0` |
| `curve25519-dalek` | `5.0.0-rc.0` | `5.0.0` |
| `ecdsa` | `0.17.0-rc.18` | `0.17.0` |
| `ed25519-dalek` | `3.0.0-rc.0` | `3.0.0` |
| `elliptic-curve` | `0.14.0-rc.33` | `0.14.1` |
| `p256` / `p384` / `p521` | `0.14.0-rc.10` | `0.14.0` |
| `primeorder` | `0.14.0-rc.10` | `0.14.0` |
| `ssh-cipher` | `0.3.0-rc.9` | `0.3.0` |
| `ssh-encoding` | `0.3.0-rc.9` | `0.3.0` |

The russh family and its own crates:

| Crate | Before | After | Note |
|---|---|---|---|
| `russh` | `0.61.2` | `0.63.3` | workspace requirement |
| `russh-sftp` | `2.3.0` | `3.0.0` | workspace requirement |
| `russh-cryptovec` | `0.61.0` | `0.62.0` | transitive only; `docs/agents/dependencies.md` §3 forbids declaring it directly, and this change does not |
| `russh-util` | `0.52.0` | `0.52.0` | unchanged |

Graph shape changes:

| Crate | Change | Why |
|---|---|---|
| `internal-russh-num-bigint 0.5.0` | **removed** | russh 0.63 uses the published `num-bigint` instead of its vendored fork |
| `num-bigint 0.5.1` | **added** | replaces the above |
| `sha3 0.12.0` | **added** | required by `russh 0.63`'s kex set |
| `sponge-cursor 0.1.0` | **added** | dependency of `sha3 0.12` |
| `wnaf 0.14.1` | **added** | dependency of the released `elliptic-curve 0.14.1` |
| `syn 3.0.5` | **added** | proc-macro dependency of the released RustCrypto crates |
| `windows-registry 0.5.3` | **removed** | dropped with `internal-russh-num-bigint`'s build script |

Incidental in-range moves the same re-resolution pulled in: `bitflags 2.13.0 -> 2.13.2`,
`bytes 1.12.0 -> 1.12.1`, `hybrid-array 0.4.12 -> 0.4.15`, `js-sys`, `log 0.4.32 -> 0.4.34`,
`rfc6979 0.5.0 -> 0.6.0`, `serde 1.0.228 -> 1.0.229`, `thiserror 2.0.18 -> 2.0.20`,
`tokio-util 0.7.18 -> 0.7.19`, `wasm-bindgen`, `web-sys`.

### Release candidates that remain — and why

| Crate | Locked | Latest on crates.io | Reason it stays |
|---|---|---|---|
| `ssh-key` | `0.7.0-rc.11` | `0.7.0-rc.11` | **no stable 0.7.0 released.** `cargo info ssh-key` reports the rc as `latest`. |
| `rsa` | `0.10.0-rc.18` | `0.10.0-rc.18` | **no stable 0.10.0 released.** |
| `pkcs1` | `0.8.0-rc.4` | `0.8.0-rc.4` | **no stable 0.8.0 released.** Pulled only by `rsa`. |

All three are upstream gaps, not OneTerm choices. `ssh-key` did move forward (`rc.10 -> rc.11`).
The count goes 16 `-rc` crates -> 3.

## Changelog items per release that touch OneTerm's surface

russh ships no `CHANGELOG.md` in the published crate, so each item below is established by
diffing the two vendored sources in the cargo registry, not by reading release notes.

### russh 0.61.2 -> 0.63.3

| # | Change | Where it lands in OneTerm |
|---|---|---|
| 1 | **OpenSSH host certificates.** New `russh::cert::PublicKeyOrCertificate` (re-exported at `russh::keys::PublicKeyOrCertificate`); `Handler::check_server_key(&mut self, &PublicKeyOrCertificate)`. `client/mod.rs:1893-1902` calls it with `Certificate` when the server presented one and with `PublicKey` otherwise — a certificate **replaces** the key check, it does not add to it. | `crates/ssh/src/handler.rs` `check_server_key` |
| 2 | **`Preferred::host_key_certificates`**, a new `Cow<'static, [Algorithm]>` field listing host-key algorithms whose `*-cert-v01@openssh.com` variant to advertise. `Preferred::DEFAULT` and `Preferred::COMPRESSED` both set it to `Cow::Borrowed(&[])`. | Nothing. OneTerm uses `client::Config::default()`, so certificates are never advertised and item 1's `Certificate` arm is unreachable on the wire. |
| 3 | **Deferred channel-open confirmation.** In 0.61.2, `client/encrypted.rs` called `confirm()?` (sending `CHANNEL_OPEN_CONFIRMATION`) *before* invoking the `server_channel_open_*` handler. In 0.63.3 it constructs a `ChannelOpenHandle` and passes it to the handler as a new next-to-last parameter; the handler calls `.accept()` or `.reject(reason)`, and `Drop` sends `ChannelOpenFailure::AdministrativelyProhibited`. The server-side `Handler::channel_open_*` methods changed the same way and their return type went `Result<bool, E>` -> `Result<(), E>`. | `crates/ssh/src/handler.rs` (2 client methods), the four in-process test servers in `crates/ssh/src`, and `crates/tools/src/bin/sftp-dev-server.rs` |
| 4 | **GSSAPI `gssapi-with-mic` auth** added: `MethodKind::GssapiWithMic`, `auth::GssapiStep`, `auth::GssapiError`, `auth::GssapiAuthenticator`, `Handle::authenticate_gssapi_with_mic`, `Handler::send_gssapi_step`. | Nothing. OneTerm never enumerates `MethodKind` exhaustively — it only calls `MethodSet::{contains, empty, from}` (`crates/ssh/src/agent.rs:158`, `crates/ssh/src/session.rs:574,929,1105,1112`). |
| 5 | **`MethodSet::all()` renamed** to `MethodSet::client_supported()`, and `MethodSet::server_supported()` added. | Nothing. OneTerm never called `all()`. |
| 6 | Internals: vendored `internal-russh-num-bigint` replaced by `num-bigint`; new `drain_priority_msgs` / `finalize_server_channel_open_reply` private hooks on the session loop. | Nothing. |

**What did *not* change** — verified by diff, and load-bearing for the risk table:

- `src/keys/known_hosts.rs` — **files are identical.** The known_hosts read, hash-match and
  append paths, and the on-disk format, are unchanged.
- `src/keys/agent/client.rs` — **files are identical.** `AgentClient`, `AgentStream`,
  `AgentIdentity`, the request-identities / sign-request flags: unchanged.
- The kex, cipher, MAC and compression preference lists
  (`SAFE_KEX_ORDER`, `CIPHER_ORDER`, `SAFE_HMAC_ORDER`, `COMPRESSION_ORDER`, negotiation.rs
  lines 105-150 in 0.61.2 vs 162-207 in 0.63.3) — **diff is empty.**
- `Preferred::DEFAULT.key` — the same seven entries in the same order (Ed25519, ECDSA
  P256/P384/P521, RSA-SHA512, RSA-SHA256, `ssh-rsa`). So `preferred_key_algorithms()` in
  `crates/ssh/src/handler.rs`, which partitions this list by what known_hosts recorded, keeps
  producing exactly the same ordering. **RSA-SHA2 negotiation is unchanged.**
- `PrivateKeyWithHashAlg`, `load_secret_key`, `AuthResult`,
  `KeyboardInteractiveAuthResponse` — signatures and semantics unchanged; every OneTerm call
  site compiles untouched.

### russh-sftp 2.3.0 -> 3.0.0

The crate is largely rewritten internally (14 of its source files changed), but the client API
OneTerm calls is source-compatible: no OneTerm SFTP call site failed to compile.

| # | Change | Where it lands in OneTerm |
|---|---|---|
| 1 | **`FileAttributes::default()` semantics inverted.** In 2.3, `Default` produced *dummy* attributes (`size: Some(0)`, `uid/gid: Some(0)`, `permissions: Some(0o777 \| FileMode::DIR)`, `atime/mtime: Some(0)`) and `empty()` produced all-`None`. In 3.0, `Default` **is** `empty()` (all-`None`) and the old dummy value moved to the new `FileAttributes::dummy()`. | One site: `crates/tools/src/bin/sftp-dev-server.rs:320`, the metadata-read fallback in `opendir`. `crates/ssh` only calls `empty()`, whose meaning is unchanged. |
| 2 | **Concurrency defaults raised.** `max_concurrent_writes` 8 -> 16; new `max_concurrent_reads: 16` and `max_write_packet_len: 32768`. Both clamped with `.max(1)`. | `crates/ssh/src/session.rs:529` uses `SftpSession::new(stream)` (defaults), so remote transfers run at twice the previous write concurrency and gain pipelined reads. Throughput only — no protocol or correctness change. |
| 3 | `SftpSession::read`/`write` convenience helpers now `close()` the file handle before returning. | `crates/ssh` opens files explicitly through `File`, so no call site changes; a leaked-handle class of bug is closed upstream. |
| 4 | `io::Error { kind: TimedOut }` now converts to `Error::Timeout` instead of `Error::IO(..)`, and `Error -> io::Error` gained a reverse impl. | `map_sftp_err` (`crates/ssh/src/sftp_task.rs:335`) matches `Error::Status(..)` and falls through on everything else, so a timeout is still `AppError::msg(..)` — only the message text changes ("Timeout" instead of an IO string). No code change. |
| 5 | New `expand-path@openssh.com` extension: `extensions::EXPAND_PATH`, `ExpandPathExtension`, `SftpSession::expand_path`, `features.expand_path`. | Nothing. Opt-in; OneTerm does not call it. |
| 6 | Read/write loop hardening: the reader and writer tasks now `break` on error and on a closed channel instead of logging and spinning. | Nothing; strictly an improvement behind `SftpSession`. |
| 7 | Serialization moved to `ser::to_packet_bytes`; `de.rs`/`ser.rs`/`buf.rs` reworked. | Nothing; internal. |

**Protocol unchanged:** `protocol::VERSION` is `3` in both releases. The wire dialect OneTerm
speaks to a server is identical.

## Crypto-crate graph, before and after

```text
BEFORE (russh 0.61.2)                     AFTER (russh 0.63.3)
---------------------                     --------------------
oneterm-ssh / oneterm-tools               oneterm-ssh / oneterm-tools
  |                                         |
  +-- russh 0.61.2                          +-- russh 0.63.3
  |     +-- ssh-key       0.7.0-rc.10       |     +-- ssh-key       0.7.0-rc.11   <-- rc, no stable
  |     |     +-- ssh-cipher  0.3.0-rc.9    |     |     +-- ssh-cipher  0.3.0      <-- stable
  |     |     +-- ssh-encoding 0.3.0-rc.9   |     |     +-- ssh-encoding 0.3.0      <-- stable
  |     |     +-- ed25519-dalek 3.0.0-rc.0  |     |     +-- ed25519-dalek 3.0.0     <-- stable
  |     |     |     +-- curve25519-dalek    |     |     |     +-- curve25519-dalek
  |     |     |            5.0.0-rc.0       |     |     |            5.0.0          <-- stable
  |     |     +-- ecdsa       0.17.0-rc.18  |     |     +-- ecdsa       0.17.0      <-- stable
  |     |     |     +-- elliptic-curve      |     |     |     +-- elliptic-curve
  |     |     |            0.14.0-rc.33     |     |     |            0.14.1         <-- stable (+ wnaf)
  |     |     +-- p256/p384/p521            |     |     +-- p256/p384/p521
  |     |     |            0.14.0-rc.10     |     |     |            0.14.0         <-- stable
  |     |     +-- primeorder 0.14.0-rc.10   |     |     +-- primeorder 0.14.0       <-- stable
  |     |     +-- rsa        0.10.0-rc.18   |     |     +-- rsa        0.10.0-rc.18 <-- rc, no stable
  |     |     |     +-- pkcs1 0.8.0-rc.4    |     |     |     +-- pkcs1 0.8.0-rc.4  <-- rc, no stable
  |     |     +-- argon2     0.6.0-rc.8     |     |     +-- argon2     0.6.0        <-- stable
  |     |     |     +-- blake2 0.11.0-rc.6  |     |     |     +-- blake2 0.11.0     <-- stable
  |     |     +-- aes-gcm    0.11.0-rc.4    |     |     +-- aes-gcm    0.11.1       <-- stable
  |     +-- internal-russh-num-bigint 0.5.0 |     +-- num-bigint 0.5.1              <-- unvendored
  |     |     +-- windows-registry 0.5.3    |     +-- sha3 0.12.0 -> sponge-cursor  <-- new
  |     +-- russh-cryptovec 0.61.0          |     +-- russh-cryptovec 0.62.0
  |     +-- russh-util      0.52.0          |     +-- russh-util      0.52.0
  +-- russh-sftp 2.3.0                      +-- russh-sftp 3.0.0
```

## Risk table

| Area | What changes | Risk | Mitigation / proof |
|---|---|---|---|
| **Host-key verification** | `check_server_key` argument type `&PublicKey` -> `&PublicKeyOrCertificate`. A naive port (`_ => Ok(true)`, or matching only the key arm and defaulting) would silently accept a host certificate OneTerm cannot verify. | **High** | The certificate arm returns an explicit refusal (`SshHandlerError::UnknownHostKey` with a `cert:` marker), never `Ok(true)`. The key arm keeps the exact `verify_server_key` body — recorded key match, changed-key refusal, algorithm-mismatch refusal, `AcceptNewFingerprint` learn, strict refusal — unchanged. Proof: the eight `handler_tests.rs` known_hosts / mismatch tests plus the loopback `check_server_key` connect test, all unmodified. |
| **Host-cert advertising** | New `Preferred::host_key_certificates`, empty by default. | Low | OneTerm never sets it and never builds a custom `Preferred`. A conforming server therefore never sends a certificate. Verified by reading `Preferred::DEFAULT` in `negotiation.rs:210`. |
| **known_hosts format / round trip** | none | None | `src/keys/known_hosts.rs` is byte-identical between the two releases. `handler_tests.rs` round-trips `learn_known_hosts_path` -> `known_host_keys_path` for ed25519, ECDSA-P256 and RSA. |
| **Key-file parsing** (OpenSSH / PKCS#8 / PEM, encrypted keys) | `ssh-key` `rc.10 -> rc.11`; `pkcs1`/`rsa` unchanged; `ecdsa`/`ed25519-dalek`/`p256` rc -> stable. `load_secret_key`'s signature is unchanged. | Medium | `US-0095` adds `crates/ssh/src/keyfile_tests.rs`: generate one key per supported algorithm (Ed25519, ECDSA P256/P384/P521, RSA-2048), write it in OpenSSH format both unencrypted and passphrase-encrypted, and assert `load_secret_key` returns the matching public key (and rejects a wrong passphrase). This path had no direct coverage before. |
| **Agent protocol** | none | None | `src/keys/agent/client.rs` is byte-identical. `agent_tests.rs` runs a real in-process `russh::keys::agent::server::serve` and authenticates against a loopback SSH server through `AgentClient`. |
| **Auth methods** | `MethodKind` gains `GssapiWithMic`; `MethodSet::all()` renamed. | Low | OneTerm calls only `contains` / `empty` / `from`; no exhaustive match exists. The keyboard-interactive `proceed_with_methods` and the `publickey_still_accepted` partial-auth logic are untouched. Proof: `session.rs` keyboard-interactive suite and `agent.rs` multi-key fallback suite. |
| **Channel / forwarding semantics** | Server-initiated channel opens are no longer pre-confirmed; the handler owns accept/reject. | Medium | Each of the two client handlers explicitly `accept()`s exactly where 0.61 would have confirmed, and drops the handle (-> `AdministrativelyProhibited`) exactly where OneTerm previously confirmed-then-closed. This makes the DEC-0011 "channels for listeners OneTerm never asked for are dropped, never bridged" rule *stronger*: the peer now gets a proper refusal instead of a confirmation followed by a close. Proof: `tunnel_tests.rs` (direct-tcpip and forwarded-tcpip over loopback), `agent_tests.rs::agent_forward_*`. |
| **Forwarding defaults (DEC-0011)** | none | None | Loopback bind, SOCKS5 CONNECT-only, agent forwarding opt-in per session are all OneTerm-side policy in `crates/ssh/src/tunnel.rs` and `handler.rs`; no russh default participates. |
| **Jump hosts (DEC-0010)** | none | None | `route.rs` chains `client::connect` / `channel_open_direct_tcpip` / `connect_stream`; all three signatures are unchanged. Proof: `route_tests.rs` two-hop relay over loopback. |
| **SFTP protocol version / extensions** | `VERSION` stays 3; `expand-path@openssh.com` added but opt-in. | None | Read from `protocol/mod.rs` in both releases. |
| **SFTP attribute defaults** | `FileAttributes::default()` flips from dummy to empty. | Medium (dev tool only) | One call site, in the dev server, changed to `FileAttributes::dummy()` to preserve behaviour. `crates/ssh` uses `empty()`, unchanged. |
| **SFTP error types** | `io::ErrorKind::TimedOut` -> `Error::Timeout`. | Low | `map_sftp_err` already falls through to `AppError::msg`; the `StatusCode` arms that the SFTP browser's permission-denied / not-found distinction depends on are unchanged. Proof: `sftp_task_tests`. |
| **SFTP throughput** | write concurrency 8 -> 16, new pipelined reads. | Low | No protocol change. A slow or strict server sees more in-flight requests; this is within the SFTP v3 window that the server itself advertises. Not verified against a real server — see Gaps. |
| **Licence / supply chain** | 5 crates added, 2 removed. | Low | `cargo deny check licenses bans advisories` and `python scripts/third-party-notices.py --check` are acceptance criteria. |

## Diagram

```text
root Cargo.toml
  russh      "0.61" --> "0.63"        russh-sftp "2.3" --> "3.0"
        |                                    |
        v                                    v
  crates/ssh/src/handler.rs            crates/ssh/src/sftp_task/**
    check_server_key(&PublicKey)         (source-compatible; no edit)
      --> (&PublicKeyOrCertificate)
    server_channel_open_agent_forward   crates/tools/src/bin/sftp-dev-server.rs
    server_channel_open_forwarded_tcpip   FileAttributes::default()
      --> + ChannelOpenHandle param        --> FileAttributes::dummy()
                                           channel_open_session + handle
        |
        v
  crates/ssh/src/{agent,route,task,tunnel}_tests.rs, session.rs
    in-process server Handlers: + ChannelOpenHandle param,
    Result<bool, E> --> Result<(), E>
```

## UI Wireframe

N/A — no UI surface. No dialog, panel, setting or message string changes. The SSH connect
dialog, the host-key prompt and the SFTP browser all render from OneTerm's own types
(`SshConnectParams`, `SshHandlerError`, `AppError`), none of which changes shape.

## Data Flow

1. Cargo resolves the two new requirements and rewrites `Cargo.lock`, pulling the stable
   RustCrypto line that `russh 0.63` requires.
2. `crates/ssh` compiles against the new `client::Handler` trait: `check_server_key` receives a
   `PublicKeyOrCertificate`, and the two server-initiated channel-open handlers receive a
   `ChannelOpenHandle`.
3. At runtime, key exchange negotiates exactly as before (identical preference lists), the
   handler unwraps the `PublicKey` arm and runs the unchanged known_hosts check, and a
   server-initiated channel is confirmed only when OneTerm actually bridges it.
4. `russh_sftp::client::SftpSession` opens on the same connection, speaking SFTP v3 as before,
   with a higher in-flight request budget.
5. `scripts/third-party-notices.py` re-reads `Cargo.lock` and rewrites the notices table;
   `cargo deny` re-checks the licences of the five added crates.

## Detail Design

- [x] Detail design: required (high-risk)
- File: [`low-level-design/upgrade.md`](low-level-design/upgrade.md) — every call site that
  changes, with before/after signatures.
- Reason: the change edits the host-key verification callback and the channel-accept path.
  Both are security-relevant, and "what exactly the new code does at each site" must be
  reviewable without reading the diff.
