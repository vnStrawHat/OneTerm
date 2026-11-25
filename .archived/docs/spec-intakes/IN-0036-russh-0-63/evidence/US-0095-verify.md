# US-0095 — independent verification

Packet: `US-0095-russh-0-63-bump.md` (intake `IN-0036`, high-risk lane)
Under review: `621e4c9..a33a994` (5 commits), russh `0.61.2 -> 0.63.3`, russh-sftp `2.3.0 -> 3.0.0`
Verifier worktree: `D:\TrungKFC-Research\Rust\myTerm2\.claude\worktrees\agent-aae034f65b1aca437`
Date: 2026-09-15
Method: upstream source diffing in the cargo registry, plus 10 new tests written by the verifier.
Nothing here is taken from the implementer's prose.

## Verdict

**PASS-WITH-NOTES.**

Every security-critical claim in the packet holds, and the two that the packet could only argue
from code reading are now proved by tests:

- the `Certificate` arm of `check_server_key` **refuses**, and it refuses *even when the
  certificate's inner key is the one recorded in known_hosts* — there is no fall-through
  (test `a_host_certificate_is_refused_even_when_its_inner_key_is_trusted`, which the packet
  listed as an untestable gap);
- the `PublicKey` arm is byte-for-byte the 0.61 logic, and the key object it receives is decoded
  by the same `parse_public_key` call as in 0.61;
- `known_hosts.rs`, `keys/agent/client.rs` and `keys/key.rs` (`PrivateKeyWithHashAlg`) are
  **identical files** between the two releases, so known_hosts semantics and agent/RSA-SHA2
  signing cannot have moved;
- the preference lists (kex / cipher / MAC / compression / host key) are byte-identical, so no
  negotiated default changed.

The notes are eight defects, all documentation-accuracy or an unreviewed throughput regression.
None of them is a correctness or security hole. **D1 and D2 change what the owner is being asked
to accept** and should be fixed in the packet before it is closed.

---

## Defects

### D1 — the SFTP write-budget claim is inverted (severity: medium; owner-facing)

- `docs/spec-intakes/IN-0036-russh-0-63/high-level-design.md:130` — "remote transfers run at
  **twice** the previous write concurrency … Throughput only — no protocol or correctness change."
- `docs/spec-intakes/IN-0036-russh-0-63/US-0095-russh-0-63-bump.md`, *Owner decision required* 2 —
  "SFTP transfers now run at **twice** the previous in-flight write budget".

Both count `max_concurrent_writes` (8 -> 16) and stop there. russh-sftp 3.0 also introduced
`Config::max_write_packet_len = 32768`, and `poll_write` now caps **every** `SSH_FXP_WRITE` at it:

```
russh-sftp-3.0.0/src/client/fs/file.rs  (poll_write)
    let preferred_write_len = self.features.max_write_packet_len
        .saturating_sub(WRITE_OVERHEAD_LENGTH + self.handle.len() as u32).max(1) as usize;
    let len = buf.len().min(packet_write_len).min(server_write_len).min(preferred_write_len);
```

russh-sftp 2.3.0 had no such cap: `len = min(buf.len(), limits.write_len)`, i.e. one packet per
`write_all` chunk. OneTerm feeds `CHUNK_LEN = 255 KiB`
(`crates/ssh/src/sftp_task/transfer/pipeline.rs:32`), so:

| | packet payload | in flight | in-flight bytes |
|---|---|---|---|
| russh-sftp 2.3.0 | 261 120 B (one per chunk) | 8 | **2 088 960 B** |
| russh-sftp 3.0.0 | **32 742 B measured** (8 per chunk) | 16 | **523 872 B** |

Measured, not inferred: a 5 MiB upload in 255 KiB chunks produced **161 WRITE packets, largest
32 742 bytes** (`a_five_mib_round_trip_matches_and_records_the_write_packet_budget`). Under
2.3.0 the same upload is 21 packets of 261 120 bytes. 4.0x less in flight, exactly.

SFTP throughput is `in-flight bytes / RTT`. The in-flight write budget **fell ~4x**, it did not
double. On loopback this is invisible; on a 100 ms-RTT link uploads should get roughly four times
slower. The owner is currently being asked to accept "faster, might upset a strict server" when
the real question is "slower on high-latency links — pin `max_write_packet_len` back up, or
accept?".

Repro: read the two `poll_write` bodies; measured in
`a_five_mib_round_trip_matches_and_records_the_write_packet_budget` (see Raw outputs).

Fix: correct both sentences; if the owner wants the old budget, the knob is
`SftpSession::new_with_config` at `crates/ssh/src/session.rs:529` with
`Config { max_write_packet_len: 262144, ..Default::default() }`.

### D2 — 3.0's read pipelining is thrown away by OneTerm's seek-per-chunk download (severity: medium)

Neither the HLD nor the LLD considers how the new read-ahead meets OneTerm's *own* striping.

russh-sftp 3.0 answers a read by queueing `max_concurrent_reads` (16) `SSH_FXP_READ` requests
ahead of the current offset:

```
russh-sftp-3.0.0/src/client/fs/file.rs  (ReadState::request)
    let count = if self.chunk_len.is_some() { features.max_concurrent_reads } else { 1 };
    while self.pending.len() < count {
        let rx = session.read_nowait(handle, self.offset, len)?;   // sent on the wire NOW
```

`read_nowait` writes the packet immediately (`rawsession.rs:451` -> `self.send(...)`). A seek
calls `ReadState::reset`, which clears the **local** queue only:

```
russh-sftp-3.0.0/src/client/fs/file.rs  (poll_seek)
    self.pos = result?;  let pos = self.pos;  self.state.read.reset(pos);
```

`crates/ssh/src/sftp_task/transfer/pipeline.rs:185 read_chunk` seeks before **every** chunk, so on
each chunk the client issues up to 16 read-aheads, consumes one, seeks away and discards the rest —
which the server has already served. Measured in-process (8 useful 255 KiB chunks on one handle):

```
US-0095 verify: 8 useful chunks (2088960 bytes) cost 29 server READ requests
                and 7573491 bytes served — amplification 3.6x
```

Data integrity is unaffected (the test byte-compares the reassembled file). The cost is bandwidth
and server load: 3.6x here, and higher on a real link where the duplex buffer is not the limiter.
OneTerm opens up to 4 such handles per download (`read_handles_for`, `READ_PIPELINE_DEPTH = 4`).

Fix options for the owner: pass `max_concurrent_reads: 1` and let OneTerm's own striping do the
pipelining (it was written for a client with no read-ahead), or drop the striping and let
russh-sftp pipeline a single sequential handle. Doing both is what produces the waste.

### D3 — `pipeline.rs`'s module doc is now false (severity: low)

`crates/ssh/src/sftp_task/transfer/pipeline.rs:9-13`:

> Writes are pipelined by the `russh-sftp` `File` itself (up to `Config::max_concurrent_writes`
> unacknowledged writes, **8 by default**) … **Reads are one-request-per-`poll_read`**, so
> `copy_striped` opens several handles …

Both statements were true of 2.3.0 and are false of 3.0.0 (16 writes; reads are 16-deep
pipelined). This is the doc that explains *why* `copy_striped` exists; leaving it stale is how D2
goes unnoticed next time. Not updated by the bump.

### D4 — DEC-0011 and `handler.rs:148` still say "closed", the code now refuses (severity: low)

The implementer updated the method doc on `server_channel_open_agent_forward` but not:

- `crates/ssh/src/handler.rs:148` — "without it every agent channel the server opens is **closed
  unanswered** (DEC-0011)";
- `docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — "the handler **closes**
  any agent channel the server opens while the switch is off".

Both now describe 0.61 behaviour. The change is strictly stronger (the peer is told
`AdministrativelyProhibited`), so this is wording, not behaviour — but DEC-0011 is an accepted
decision record that future work reads as the rule.

### D5 — the LLD's `check_server_key` sample does not match the shipped code (severity: low)

`docs/spec-intakes/IN-0036-russh-0-63/low-level-design/upgrade.md`, Change A:

```rust
fingerprint: format!("cert:{}", certificate.fingerprint(HashAlg::Sha256)),
```

shipped (`crates/ssh/src/handler.rs:337`):

```rust
fingerprint: format!("cert:{}", certificate.public_key().fingerprint(HashAlg::Sha256)),
```

These are different values (the certificate blob vs the key inside it). The shipped one is the
better choice — it is the fingerprint an operator can compare against `known_hosts` — but the LLD
is the record of what was built.

### D6 — the docs overstate the new key-file coverage (severity: low)

LLD Change E and `high-level-design.md:179` both say the new tests "generate a `PrivateKey`" for
"Ed25519, ECDSA P256/P384/P521 **and RSA-2048**" and encrypt "**for Ed25519 and RSA-2048**".
The shipped `crates/ssh/src/keyfile_tests.rs` uses a **fixed RSA fixture** (generation is too slow
in a debug build — the right call, stated in the file) and encrypts **only** Ed25519. There is no
encrypted-RSA test. This verification adds one
(`an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase`, passing).

### D7 — "no stable release on crates.io at all" is false, and the real reason is stronger (severity: low)

`US-0095-russh-0-63-bump.md`, *Owner decision required* 1: "These three have no stable release on
crates.io at all". `rsa 0.9.x`, `pkcs1 0.7.x` and `ssh-key 0.6.x` are stable releases; what has no
stable release is each crate's *current major line*. The decisive fact, which the packet does not
state, is that **russh 0.63.3 pins all three exactly**:

```
russh-0.63.3/Cargo.toml:281   [dependencies.pkcs1]   version = "=0.8.0-rc.4"
russh-0.63.3/Cargo.toml:309   [dependencies.rsa]     version = "=0.10.0-rc.18"
russh-0.63.3/Cargo.toml:351   [dependencies.ssh-key] version = "=0.7.0-rc.11"
```

With `=` requirements, cargo gives OneTerm no choice at all — a stronger argument than the one
made. `cargo info` confirms each rc is the crate's newest published version.

### D8 — a certificate refusal still opens the "trust this host key?" dialog (severity: low)

The Certificate arm returns `SshHandlerError::UnknownHostKey`, which
`SshHandlerError::to_app_error` maps to `AppError::HostKeyUnknown`
(`crates/ssh/src/handler.rs:102`), which the UI answers with the first-use approval dialog
(`crates/session-ui/src/common.rs:393` -> `open_host_key_confirmation`).

There is **no trust bypass**: approving retries with
`HostKeyPolicy::AcceptNewFingerprint("cert:…")`, and the Certificate arm returns `Err`
*unconditionally, before any policy check*, so the connection is refused again. But the user is
shown an Accept button that can never succeed, and `docs/ssh-client-connect.md` §9.3's phrase
"refused, and not approvable from the UI" reads as though no dialog appears. Either give the
certificate case its own error variant, or say in §9.3 that the dialog appears and approving it
fails again.

### N9 — informational: five client channel-open hooks still auto-accept (not a defect)

The packet says "OneTerm's 'never asked for this' paths now refuse instead of confirm-then-close".
That is true of the two hooks OneTerm overrides. In russh 0.63.3 the **client-side** defaults for
the other five **accept**:

```
russh-0.63.3/src/client/mod.rs ~2477-2600
    fn server_channel_open_session(...)            -> async move { reply.accept().await; Ok(()) }
    fn server_channel_open_x11(...)                -> async move { reply.accept().await; Ok(()) }
    fn server_channel_open_direct_tcpip(...)       -> async move { reply.accept().await; Ok(()) }
    fn server_channel_open_direct_streamlocal(...) -> async move { reply.accept().await; Ok(()) }
    fn server_channel_open_forwarded_streamlocal(..)->async move { reply.accept().await; Ok(()) }
```

(Only `server_channel_open_unknown` drops the handle, and it is gated behind
`should_accept_unknown_server_channel`, whose default is `false`.) OneTerm implements only
`check_server_key`, `server_channel_open_agent_forward` and
`server_channel_open_forwarded_tcpip` (`crates/ssh/src/handler.rs:312-410`), so a server-initiated
session / x11 / direct-tcpip / streamlocal channel is still confirmed and then dropped — exactly
what 0.61 did. **No regression**, but the fail-closed posture is narrower than the packet's
sentence suggests, and five one-line `reply.reject(...)` overrides would close it.

Note the LLD's Change C statement ("russh's default implementation … drops the handle and
therefore rejects") is correct for the **server** trait, which is what Change C is about —
`russh-0.63.3/src/server/mod.rs:362,377,395,417` are all `async { Ok(()) }`. It simply does not
hold on the client side.

### Positive finding the packet does not claim

0.63.3 fixes three panics a hostile server could reach in 0.61.2, and hardens one comparison:

```
kex/mod.rs:486      encode_mpint on an all-zero shared secret indexed s[s.len()] -> panic
                    (an attacker-chosen all-zero Curve25519 point); now encodes mpint 0.
cipher/mod.rs:317   a packet_length shorter than the probe block underflowed the
                    buffer[l..] slice -> panic; now Error::PacketSize.
keys/format/pkcs8_legacy.rs:220  clone_from_slice on an IV that was not 16 bytes -> panic;
                    now Error::InvalidParameters.
keys/agent/server.rs:251  agent unlock password compared with ==; now subtle::ct_eq.
```

Remote-crash fixes are a stronger reason to take this bump than "leave the `-rc` pins", and
belong in the packet's case.

---

## Raw outputs

### Upstream diff sizes (unified, CRLF-normalised)

```
$ diff -u 0.61.2/src/keys/known_hosts.rs   0.63.3/src/keys/known_hosts.rs    -> 0 lines (identical)
$ diff -u 0.61.2/src/keys/agent/client.rs  0.63.3/src/keys/agent/client.rs   -> 0 lines (identical)
$ diff -u 0.61.2/src/keys/key.rs           0.63.3/src/keys/key.rs            -> 0 lines (identical)
$ diff -u 0.61.2/src/negotiation.rs        0.63.3/src/negotiation.rs         -> 684 lines
$ diff -u 0.61.2/src/client/mod.rs         0.63.3/src/client/mod.rs          -> 701 lines
$ diff -u 0.61.2/src/kex/mod.rs            0.63.3/src/kex/mod.rs             -> 57 lines
$ diff -u 0.61.2/src/cipher/mod.rs         0.63.3/src/cipher/mod.rs          -> 17 lines
$ diff -r -q 0.61.2/src/keys/  0.63.3/src/keys/
    only agent/server.rs, format/pkcs8_legacy.rs and mod.rs differ
$ diff -u --strip-trailing-cr 2.3.0/src/client/fs/file.rs 3.0.0/src/client/fs/file.rs -> 449 lines
```

`keys/mod.rs`'s entire diff is `+pub use crate::cert::PublicKeyOrCertificate;`.

### Preference lists — identical

```
0.61.2/src/negotiation.rs:105 SAFE_KEX_ORDER   == 0.63.3/src/negotiation.rs:162  (13 entries)
0.61.2:128 CIPHER_ORDER      == 0.63.3:185  (chacha20-poly1305, aes256-gcm, aes256/192/128-ctr)
0.61.2:137 SAFE_HMAC_ORDER   == 0.63.3:194  (hmac-sha2-512/256-etm, hmac-sha2-512/256)
0.61.2:144 COMPRESSION_ORDER == 0.63.3:201  (none, zlib, zlib@openssh.com)
Preferred::DEFAULT.key: 0.61.2:155 == 0.63.3:213 — 7 entries, same order.
```

**`kex::MLKEM768X25519_SHA256` is the first entry in `SAFE_KEX_ORDER` in BOTH releases**
(`0.61.2/src/negotiation.rs:106` and `0.63.3/src/negotiation.rs:163`). The
`mlkem768x25519-sha256` the implementer saw negotiated is **not new in 0.63** and is not an
interop change; it was already OneTerm's first choice on 0.61.2.

The only field added to `Preferred` is the one that matters here, and it is empty:

```
russh-0.63.3/src/negotiation.rs:212   host_key_certificates: Cow::Borrowed(&[]),   // Preferred::DEFAULT
russh-0.63.3/src/negotiation.rs:239   host_key_certificates: Cow::Borrowed(&[]),   // Preferred::COMPRESSED
```

and OneTerm's only `client::Config` (`crates/ssh/src/route.rs:52-60`) sets `preferred.key` and
nothing else, so a conforming server is never offered a certificate algorithm.

### Host-key path — upstream semantics

```
russh-0.63.3/src/client/mod.rs:1893
    if let Some(certificate) = server_host_certificate {
        if !handler.check_server_key(&certificate.into()).await? { return Err(Error::UnknownKey) }
    } else if let Some(server_host_key) = server_host_key {
        let check = handler.check_server_key(&server_host_key.into()).await?;
```

A certificate **replaces** the key check upstream; falling through to the inner key would have
been a real hole, and OneTerm does not. The `PublicKey` arm receives the value of
`parse_public_key(&blob)` (`0.63.3/src/client/kex.rs:304`), the same call 0.61.2 made
(`0.61.2/src/client/kex.rs:267`), wrapped by `From<PublicKey>` with `hash_alg: None`. Discarding
`hash_alg` in `crates/ssh/src/handler.rs:329` is therefore a no-op, and `verify_server_key` is
untouched by the diff.

### `ChannelOpenHandle` — drop really does refuse

```
russh-0.63.3/src/lib_inner.rs:610   impl Drop for ChannelOpenHandleInner
    Err(ChannelOpenFailure::AdministrativelyProhibited) sent on the priority channel.
russh-0.63.3/src/client/mod.rs:1358,1378,1387  every select arm calls drain_priority_msgs()
    before handling channel data, so an accept() is always written before the bridge's bytes.
```

The implementer's "confirm before the bridge writes anything" ordering is sound.

### `FileAttributes::default()` audit (every use in the workspace)

```
$ grep -rn "FileAttributes" crates/ --include=*.rs
crates/ssh/src/sftp_task/metadata.rs:7,120,124        type only
crates/ssh/src/sftp_task/transfer/download.rs:7,80,218 type only
crates/ssh/src/sftp_task/transfer.rs:8,45,94           type only
crates/ssh/src/sftp_task/transfer.rs:105               ..FileAttributes::empty()   <- meaning unchanged
crates/tools/src/bin/sftp-dev-server.rs:324            FileAttributes::dummy()     <- the one changed site
$ grep -rn "Default::default" crates/ssh crates/sftp-ui crates/tools --include=*.rs
crates/sftp-ui/src/edit.rs:1048,1056,1064,1073         notify::Event::attrs — unrelated crate
crates/ssh/src/{handler_tests,route,test_support}.rs   russh::client::Config
crates/tools/src/bin/sftp-dev-server.rs:517            russh::server::Config
crates/tools/src/bench.rs:451, bin/pty-throughput.rs:37 unrelated
```

Exactly one site relied on the flipped `Default`, and it was changed. `transfer.rs:105`'s
`empty()` is correct: an `apply_local_metadata_to_remote` setstat must carry only the attributes
OneTerm means to set. Confirmed correct.

### Tests written for this verification

`crates/ssh/src/us0095_verify_tests.rs` (wired from `crates/ssh/src/lib.rs`), 11 tests, all pass.
They are kept in the verifier's worktree and are **not committed**:

```
a_recorded_host_key_is_accepted_through_the_handshake
an_unrecorded_host_key_is_unknown_through_the_handshake
a_changed_host_key_is_refused_as_changed_not_unknown
a_host_certificate_is_refused_even_when_its_inner_key_is_trusted   <- closes the packet's gap
an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase
a_pkcs8_pem_ed25519_key_loads
a_key_file_with_crlf_line_endings_records_its_outcome
an_empty_passphrase_on_an_encrypted_key_is_refused
a_truncated_key_file_errors_rather_than_panics
seek_per_chunk_reads_measure_the_new_pipeline_read_ahead
a_five_mib_round_trip_matches_and_records_the_write_packet_budget
```

The certificate test builds a real self-signed host certificate
(`ssh_key::certificate::Builder`, `CertType::Host`), serves it from a loopback `russh::server`
with `Config::certificates`, forces negotiation by setting
`preferred.host_key_certificates = [Algorithm::Ed25519]` on the **client**, and — the adversarial
part — **pre-records the certificate's inner key in `known_hosts` first**, so a fall-through to a
key comparison would have accepted. It does not.

Results:

```
$ cargo test -p oneterm-ssh --lib us0095 -- --nocapture --test-threads=1
running 11 tests
test us0095_verify_tests::a_changed_host_key_is_refused_as_changed_not_unknown ... ok
US-0095 verify: 5 MiB round trip OK. uploaded in 161 WRITE packets (largest 32742 bytes,
                5242880 bytes total); downloaded in 21 READ packets (5242880 bytes).
test us0095_verify_tests::a_five_mib_round_trip_matches_and_records_the_write_packet_budget ... ok
test us0095_verify_tests::a_host_certificate_is_refused_even_when_its_inner_key_is_trusted ... ok
US-0095 verify: a CRLF OpenSSH key file LOADS
test us0095_verify_tests::a_key_file_with_crlf_line_endings_records_its_outcome ... ok
test us0095_verify_tests::a_pkcs8_pem_ed25519_key_loads ... ok
test us0095_verify_tests::a_recorded_host_key_is_accepted_through_the_handshake ... ok
test us0095_verify_tests::a_truncated_key_file_errors_rather_than_panics ... ok
test us0095_verify_tests::an_aes256_ctr_bcrypt_rsa_key_loads_with_its_passphrase ... ok
test us0095_verify_tests::an_empty_passphrase_on_an_encrypted_key_is_refused ... ok
test us0095_verify_tests::an_unrecorded_host_key_is_unknown_through_the_handshake ... ok
US-0095 verify: 8 useful chunks (2088960 bytes) cost 29 server READ requests
                and 7573491 bytes served — amplification 3.6x
test us0095_verify_tests::seek_per_chunk_reads_measure_the_new_pipeline_read_ahead ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 73 filtered out
```

The 5 MiB round trip is item 6's transfer check, done in-process rather than through
`sftp-dev-server` (which needs an external SSH client that cannot authenticate in this
environment — the same wall the implementer hit). Both directions byte-compare equal, so
**data integrity through the new SFTP client is proved**. The packet counts are the measurement
behind D1 and D2:

| | packets | largest packet | bytes on the wire |
|---|---|---|---|
| upload, 5 MiB, 255 KiB chunks | **161** WRITE | **32 742 B** | 5 242 880 (no waste) |
| download, 5 MiB, sequential | 21 READ | — | 5 242 880 (no waste) |
| download, 2 MiB, **seek per chunk** | **29** READ for 8 useful | — | **7 573 491** (3.6x) |

161 write packets for what russh-sftp 2.3.0 would have sent in 21 (5 242 880 / 261 120) is the
32 KiB cap, measured. And the sequential download costs nothing extra — the amplification in the
third row is caused specifically by the **seek before every chunk** that `copy_striped` does, not
by pipelining itself. That is the precise diagnosis for D2: OneTerm's striping and russh-sftp's
read-ahead are two solutions to the same problem, and running both wastes the second.

Note on the CRLF case: it **loads**. `decode_secret_key` compares whole lines, but Rust's
`str::lines()` strips a trailing `\r`, so CRLF key files are fine — and that file
(`src/keys/format/mod.rs`) is byte-identical between the two releases anyway.

### Remote-forward accept/refuse (item 3)

Not duplicated: `crates/ssh/src/tunnel_tests.rs:a_remote_forward_reaches_the_local_target_and_unknown_ports_are_dropped`
already asserts **both** halves after the implementer's edit — a requested forward delivers bytes
to the local target, and `channel_open_forwarded_tcpip("127.0.0.1", 9001, …)` for an
unrequested port now returns `Err` to the server (it returned `Ok` + close on 0.61). Re-run and
passing. Every client hook that can receive a server-initiated open is accounted for in N9.

### Gate runs

```
$ cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools
oneterm-ssh     : 73 passed; 0 failed; 0 ignored
oneterm-sftp-ui : 49 passed; 0 failed; 0 ignored
oneterm-tools   : 14 + 2 passed; 0 failed; 0 ignored
  -> matches the packet's 73 / 49 / 16 exactly.

$ pwsh scripts/ci-local.ps1 --full          (on the clean tree, verifier's tests set aside)
...
advisories ok, bans ok, licenses ok
ci-local: all checks passed.                                          [exit 0]

$ cargo test --workspace                    -> 56 sections, 1611 passed, 0 failed, 11 ignored
$ cargo test -p oneterm-vt --features vt-paranoid
                                            ->  4 sections,  370 passed, 0 failed,  3 ignored
                                            =  60 / 1981 / 0 / 14
  -> reproduces the packet's 60 / 1981 / 0 / 14 exactly.

$ cargo deny check licenses bans advisories  (the --full step above)
advisories ok, bans ok, licenses ok         -> 0 yanked, as the acceptance criterion requires.
$ python scripts/third-party-notices.py --check   -> passed (inside ci-local)
$ python scripts/verify-dependency-graph.py       -> passed (inside ci-local)
```

The verifier's own test file was moved out of the tree for the ci-local run so the totals are
the implementer's, not inflated by the 11 tests added here.

### `-rc` crates remaining

```
$ grep -B2 'version = ".*-rc' Cargo.lock | grep 'name ='
name = "pkcs1"    0.8.0-rc.4
name = "rsa"      0.10.0-rc.18
name = "ssh-key"  0.7.0-rc.11
$ cargo info ssh-key / rsa / pkcs1  -> 0.7.0-rc.11 / 0.10.0-rc.18 / 0.8.0-rc.4 (each is newest)
russh-0.63.3/Cargo.toml pins all three with '=' (see D7). 16 -rc crates -> 3 confirmed.
```

### Commit trailers — all five correct

```
326fc2e docs(ssh): record IN-0036 / US-0095 for the russh 0.63 bump
0872d84 build(deps): move the russh family to 0.63 / 3.0
44c62a5 fix(ssh): follow russh 0.63 on the host-key and channel-open callbacks
167253c fix(tools): follow russh 0.63 and russh-sftp 3.0 in sftp-dev-server
a33a994 docs(ssh): complete US-0095 with the russh 0.63 upgrade evidence
  each carries:
    Refs: IN-0036, US-0095
    Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
    Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

### Docs accuracy (item 9)

- `docs/ssh-client-connect.md` §9.3 — the new row is accurate about the refusal, the `cert:`
  prefix, the empty `host_key_certificates` and the missing CA trust store. See D8 for the one
  phrase to soften.
- `docs/agents/dependencies.md` §3 — accurate: version ranges, `default-features = false`, the
  feature list, and "`russh-cryptovec` and `russh-util` follow transitively and are never
  declared" all match root `Cargo.toml` and `Cargo.lock`.
- LLD `upgrade.md` — the call-site list is **complete and correct**: every file and method it
  names appears in `git diff 621e4c9..a33a994`, and the diff contains no site the LLD omits.
  Defects D5 and D6 are inaccuracies *within* entries, not missing entries.
- "Owner decision required" — the two items are the ones the implementer reported. **Add D1's
  correction (the write budget fell, it did not double) and D2 (the read-ahead OneTerm discards)
  to that list**; they are the two things the owner is actually being asked to accept about SFTP,
  and D7's correction to item 1.

---

## What changes for a user

Nothing a user does changes, and nothing they trust gets weaker.

Connecting to a server works exactly as before: the same host-key prompt the first time, the same
flat refusal if a known host's key changes, the same key files and passphrases, the same agent
and jump-host behaviour. Under the hood the client now negotiates the identical algorithms it
negotiated before — the post-quantum key exchange OneTerm uses was already the default on the old
version, so no server will behave differently.

Two things are genuinely better. A server that opens a channel OneTerm never asked for — an agent
channel when agent forwarding is off, or a forwarded connection for a port that was never
requested — is now **told no** instead of being let in and immediately hung up on. And the new
library fixes three ways a malicious server could crash OneTerm's connection outright, which the
old one did not.

One thing is a genuine host-certificate refusal: if a server ever presents an OpenSSH *host
certificate* instead of a plain key, OneTerm refuses it rather than guessing. It cannot happen
with a well-behaved server — OneTerm never asks for certificates — and a test now proves that even
when the certificate wraps a key OneTerm already trusts, the connection is still refused. The one
rough edge is that the refusal reaches the user through the ordinary "trust this host key?" dialog,
whose Accept button will simply fail again; it never lets anything through, it just does not
explain itself well.

The one thing the owner should decide about is file transfer speed. The new library changed how it
paces SFTP requests, and the verification measured both directions on a 5 MB file. **Uploads now
go out in 161 small packets where the old version sent 21 large ones** — a quarter as much data
waiting on the wire at any moment, which on a distant server means uploads roughly four times
slower. **Downloads** are fine when read straight through, but OneTerm splits a download across
several parallel readers, and that pattern makes the new library ask the server for about 3.6
times more data than OneTerm keeps — wasted bandwidth on a metered or slow link.

On a local network nobody will notice either effect, and nothing is broken: the 5 MB file came
back byte-for-byte identical in both directions. Both are one-line configuration changes to tune.
But the packet currently tells the owner transfers got *faster*, and that is the wrong way round —
that sentence is what the owner should be asked to accept, corrected.

---

# Re-verification of 41f7539

Rework under review: `a33a994..41f7539`, four commits (`9501bab`, `c9d1cb3`, `2204235`,
`41f7539`). Same worktree, reset to `41f7539`. Date: 2026-09-15.

## Verdict: **PASS**

All eight defects are addressed, each fix is verified against the upstream source and by
measurement rather than against the implementer's description, and every gate reproduces. Two
residual nits (R1, R2) are cosmetic inaccuracies *inside* tables that now understate how good the
result is; neither blocks completion.

The verifier's 11 tests were adopted with one intentional change (the certificate test now expects
`SshHandlerError::HostCertificate` and additionally asserts the `to_app_error()` mapping). That
change is correct and strengthens the test: the adversarial setup — pre-recording the
certificate's inner key in `known_hosts` — is preserved intact.

---

## 1. D1 — the write budget (verified, and the "strict server" question answered)

`crates/ssh/src/session.rs sftp_config()` sets `max_write_packet_len: 262_144`,
`max_concurrent_writes: 8`, `max_concurrent_reads: 1`, and `open_sftp` now calls
`SftpSession::new_with_config(stream, sftp_config())`.

Re-measured, 5 MiB loopback upload in 255 KiB chunks:

```
US-0095 F: 5 MiB upload in 21 WRITE packets (largest 261120 B);
           in-flight budget 2088960 B vs russh-sftp 2.3.0's 2088960 B
```

**21 packets, largest 261 120 B, 2 088 960 B in flight — the 2.3.0 baseline exactly**, against
161 / 32 742 / 523 872 at `a33a994`. D1 fixed.

### Can our cap ever exceed what a strict server allows? **No.**

Read in russh-sftp 3.0.0; four independent clamps exist and the change relaxes only the one that
russh-sftp added for itself:

```
client/session.rs:74-76   (new_with_config)
    if let Some(plen) = limits.packet_len {
        features.max_packet_len = (plen as u32).min(max_packet_len);
    }
      -> the SERVER's advertised packet length clamps ours. Our 262 144 is a ceiling, not a floor.

client/fs/file.rs:374-395 (poll_write)
    packet_write_len    = features.max_packet_len       - (25 + handle.len())
    server_write_len    = features.limits.write_len     (the server's limits@openssh.com reply)
    preferred_write_len = features.max_write_packet_len - (25 + handle.len())
    len = buf.len().min(packet_write_len).min(server_write_len).min(preferred_write_len)
      -> `min` of all four. Raising max_write_packet_len to max_packet_len makes
         preferred_write_len == packet_write_len, i.e. NON-BINDING. It cannot lift the others.

client/rawsession.rs:431-437 (write_nowait_from_slice)
    if limits.write_len.is_some_and(|limit| data.len() as u64 > limit) {
        return Err(Error::Limited("write limit reached"))
    }
      -> a second, independent refusal below poll_write.
```

Against OpenSSH (`limits@openssh.com` `max-write-length = 261 120`) the binding term is
`server_write_len`, and `buf.len()` = `CHUNK_LEN` = 261 120 sits exactly on it — which is why the
measurement shows 261 120 and not the packet ceiling. This is byte-for-byte russh-sftp 2.3.0's
arithmetic (2.3.0 had only the first two terms), so OneTerm is back to what it shipped for the
product's entire life.

One residual edge, **unchanged by this fix and not caused by it**: a server that does *not*
advertise `limits@openssh.com` leaves `server_write_len = u32::MAX`, so the only cap is
`max_packet_len - 25 - handle` ≈ 262 KiB. russh-sftp 2.3.0 behaved identically. Worth knowing it
is pre-existing, not new.

## 2. D2 — the read budget, the residual, and the regression test

Re-measured, 5 MiB loopback download, all three configurations in one test:

```
US-0095 G: 5 MiB download, 5242880 bytes wanted
  control (striping + read-ahead 16): 201 READs, 48700595 B (9.29x), 60.8839ms
  A (striping + read-ahead 1)       :  21 READs,  5263100 B (1.00x),  8.119ms
  B (no striping + read-ahead 16)   :  21 READs,  5242880 B (1.00x),  7.7462ms
```

**A is what ships: 21 READs, 1.00x.** Note the control is **9.29x**, worse than the 3.6x this
verification first measured — confirming the earlier caveat that 3.6x was a floor set by the
duplex buffer, not a ceiling.

### The residual +20 220 B checks out exactly

`5 263 100 - 5 242 880 = 20 220 = 20 x 1 011`, and `1 011 = 262 131 - 261 120` where
`262 131 = max_packet_len(262 144) - READ_OVERHEAD_LENGTH(13)`
(`client/fs/file.rs ReadState::request`). russh-sftp asks for a full packet's worth; OneTerm's
`read_exact` consumes `CHUNK_LEN`; the seek to the next stripe drops the 1 011-byte tail. Twenty
mid-file seeks, not twenty-one — the last chunk is the tail and has nothing after it to discard.
**0.39 % overhead, arithmetic rather than read-ahead, and russh-sftp 2.3.0 read the same way.**
The explanation in the packet and in `pipeline_budget_tests.rs` is sound.

### The regression test bites in both directions

`crates/ssh/src/sftp_task/transfer/pipeline_budget_tests.rs`:

```rust
assert_eq!(a_reads, SIZE.div_ceil(CHUNK_LEN), "... russh-sftp's read-ahead is back on ...")
assert!(a_served <= SIZE + slack * SIZE.div_ceil(CHUNK_LEN))          // slack = 262_131 - CHUNK_LEN
assert!(control_served > SIZE, "expected 3.0's defaults to over-read under seek-per-chunk")
assert!(in_flight >= BASELINE_IN_FLIGHT_WRITE_BYTES)                  // 8 * 261_120
assert_eq!(packets, SIZE.div_ceil(CHUNK_LEN))                         // one packet per chunk
```

It asserts **one READ per chunk** *and* keeps a live control proving the default still amplifies —
so if someone restores `max_concurrent_reads: 16` the first assertion fails, and if a future
russh-sftp removes the read-ahead the control assertion fails and the test stops claiming
something it no longer measures. Both halves are load-bearing. The tests import
`session::sftp_config` and `super::CHUNK_LEN` directly, so the production path and the assertion
cannot drift apart.

### Is deferring option B to a follow-up sound? **Yes, and the packet's reasoning is right.**

B is genuinely better on a latent link and the packet says so (16 reads of ~256 KiB ≈ 4.2 MB in
flight against the striping's 4 x 261 120 ≈ 1.04 MB — reads have no 32 KiB cap, only writes do).
It must nonetheless **not** ship here: `copy_striped` is not only a pipelining device, it also
owns three behaviours a single `read_to_end` does not re-establish —

- per-chunk cancellation (`pipeline.rs:138-145`, `tokio::select! { biased; _ = cancel.cancelled()
  => { in_flight.abort_all(); return Err(AppError::Cancelled) } }`) — a `read_to_end` inside
  russh-sftp cannot be interrupted at the same granularity;
- progress cadence: `on_bytes(copied)` fires once per 255 KiB chunk in write order
  (`pipeline.rs:169-177`), which is what the transfer progress bar is calibrated against;
- the shrinking-remote-file clamp (`pipeline.rs:161-166`, `chunk_count.min(index + 1)`).

Folding those into a dependency bump would make the bump a transfer-engine rewrite in the
high-risk lane. Deferring with the measurement already recorded is the correct call, and the
follow-up starts with numbers instead of a hypothesis.

## 3. D8 — no trust prompt is reachable for a certificate

Traced exhaustively, not sampled:

```
$ grep -rn "open_host_key_confirmation" crates/ --include=*.rs
crates/session-ui/src/common.rs:401      <- the one and only call
crates/session-ui/src/common.rs:431      <- its definition
```

`common.rs:401` sits inside `Err(AppError::HostKeyUnknown { .. })` at `common.rs:393`. After the
fix `AppError::HostKeyUnknown` is produced at exactly one place — `handler.rs:127`, the
`UnknownHostKey` arm fed only by `verify_server_key`'s bare-key path. The certificate arm
(`handler.rs:366`) now returns the new `SshHandlerError::HostCertificate`, which
`to_app_error` maps at `handler.rs:153` to `AppError::Connect { phase: Transport, .. }`; that
falls to the generic `Err(error)` arm and becomes an error notification. Both connect paths map
through `to_app_error` (`route.rs:74` direct, `route.rs:89` jump hop), so a jump hop behaves the
same.

The adopted test asserts both halves — the variant *and* the `AppError` mapping — while keeping
the adversarial setup that makes it meaningful:

```
test us0095_verify_tests::a_host_certificate_is_refused_even_when_its_inner_key_is_trusted ... ok
```

`docs/ssh-client-connect.md` §9.3's rewritten row now matches the code exactly and cites this
test. D8 fixed.

## 4. D3-D7 and the informational items — all accurate

| | check | result |
|---|---|---|
| D3 | `pipeline.rs:9-23` module doc | Correct: names `sftp_config` as the owner of both numbers, says reads are one-at-a-time per handle, quotes the **9.3x** figure (the 5 MiB measurement, not the stale 3.6x), and warns to raise `max_concurrent_reads` if `copy_striped` is ever retired. |
| D4 | DEC-0011 | Correct ADR hygiene: the **Decision block is untouched**; a dated mechanism update is appended under **Consequences** saying the decision is unchanged and now enforced more strictly, read "closes" as "refuses". `handler.rs:180-183` field doc updated to match. |
| D5 | LLD fingerprint line | Fixed to `certificate.public_key().fingerprint(HashAlg::Sha256)`, matching `handler.rs:370`. The added note that "`Certificate` has no `fingerprint()` of its own" is **true** — verified in `ssh-key-0.7.0-rc.11`: `fingerprint()` exists on `PrivateKey` (`private.rs:573`), `PublicKey` (`public.rs:382`) and `KeyData` (`public/key_data.rs:116`), and on no certificate type. |
| D6 | keyfile coverage wording | Fixed and now matches the shipped file exactly: generated Ed25519 + P-256/384/521, RSA as a fixed `ssh-keygen` fixture with the reason, encrypted **Ed25519 only**, and encrypted RSA credited to the adopted suite. |
| D7 | the `=` pin reason | Fixed in `dependencies.md` and the packet; the false "no stable release exists at all" is gone. Verified against `russh-0.63.3/Cargo.toml:281` (`pkcs1 = "=0.8.0-rc.4"`), `:309` (`rsa = "=0.10.0-rc.18"`), `:351` (`ssh-key = "=0.7.0-rc.11"`). |
| panics | HLD:120-123 | All four file:line references match the upstream diffs: `kex/mod.rs:486` all-zero mpint, `cipher/mod.rs:317` packet-length underflow, `keys/format/pkcs8_legacy.rs:220` IV `clone_from_slice`, `keys/agent/server.rs:251` `==` -> `subtle::ct_eq`. |
| 2-of-7 hooks | LLD Change B addendum | Accurate, including the asymmetry: client defaults **accept** (`client/mod.rs` ~2477-2600), server defaults **reject** (`server/mod.rs:362,377,395,417` are `async { Ok(()) }`), so Change C's original sentence was right about the server trait and wrong only if read as a client claim. Correctly framed as out of scope rather than silently fixed. |

## 5. Gates

```
$ cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools
oneterm-ssh     : 86 passed; 0 failed; 0 ignored     (73 + 11 adopted + 2 budget)
oneterm-sftp-ui : 49 passed; 0 failed; 0 ignored
oneterm-tools   : 14 + 2 passed; 0 failed            = 86 / 49 / 16, as claimed.

$ pwsh scripts/ci-local.ps1 --full
advisories ok, bans ok, licenses ok
ci-local: all checks passed.                                          [exit 0]

$ cargo test --workspace                          -> 56 sections, 1624 passed, 0 failed, 11 ignored
$ cargo test -p oneterm-vt --features vt-paranoid ->  4 sections,  370 passed, 0 failed,  3 ignored
                                                  =  60 / 1994 / 0 / 14, as claimed (+13 on 1981).

$ git log a33a994..41f7539  -- trailers on all four commits:
9501bab test(ssh): adopt the US-0095 independent verification suite
c9d1cb3 fix(ssh): keep russh-sftp 2.3.0 transfer budget instead of 3.0 defaults
2204235 fix(ssh): refuse a host certificate as a connect error, not a trust prompt
41f7539 docs(ssh): correct US-0095 records against the independent verification
  each: Refs: IN-0036, US-0095
        Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>
        Claude-Session: https://claude.ai/code/session_01Q6xr5jX29B2b6L4MGsoNdW
```

## Residual nits (not blocking)

**R1.** `low-level-design/upgrade.md`, Change G's table says "measured on the same 5 MiB loopback
download", but the control row carries the earlier **2 MiB** figures ("3.6x the file", "29 per 8
useful"). The 5 MiB control is **9.29x / 201 READs**, which the packet's PROOF has correctly. The
table therefore *understates* the defect that was fixed; align it with PROOF.

**R2.** Same file, Change F's table gives the after row as "261 095 B / 2 088 760". The measured
values are **261 120 B / 2 088 960** — `buf.len()` (`CHUNK_LEN`), not the packet ceiling, is the
binding term — which is the 2.3.0 baseline *exactly*, not 200 bytes under it. The table is
conservative; the real result is an exact restoration.

**R3 (note only).** The adopted `us0095_verify_tests::a_five_mib_round_trip_…` and
`…seek_per_chunk_reads_…` construct `russh_sftp::client::Config::default()` directly, so their
printed numbers (161 WRITE packets, 3.6x) describe the **pre-fix** library defaults, not OneTerm's
shipped behaviour. That is deliberate and correct — they are the "before" half of the measurement,
and `pipeline_budget_tests` asserts the "after" — but a reader who greps for "161 WRITE packets"
could misread them as current. The `US-0095 F`/`US-0095 G` labels on the budget tests make the
distinction clear enough to leave alone.

---

## What changes for a user — updated for the shipped budget

Nothing a user does changes, and nothing they trust gets weaker.

Connecting to a server works exactly as before: the same host-key prompt the first time, the same
flat refusal if a known host's key changes, the same key files and passphrases, the same agent and
jump-host behaviour. The client negotiates the identical algorithms it negotiated before — the
post-quantum key exchange was already the default on the old version — so no server behaves
differently.

**File transfers keep the speed they had.** The new library would have quietly made uploads about
four times slower on a distant server and made downloads ask for roughly nine times more data than
OneTerm actually keeps. Both are now pinned back to the behaviour OneTerm has always shipped, and
measured to prove it: a 5 MB upload goes out in 21 large packets exactly as before, and a 5 MB
download costs the server 21 reads and 5 263 100 bytes for a 5 242 880-byte file — a 0.4 % overhead
that the old version paid too. A test now fails if a future library update takes either budget
away again.

Two things are genuinely better. A server that opens a channel OneTerm never asked for — an agent
channel when agent forwarding is off, or a forwarded connection for a port that was never
requested — is now **told no** instead of being let in and immediately hung up on. And the new
library fixes three ways a malicious or broken server could crash OneTerm's connection outright.

One new refusal: if a server ever proves its identity with an OpenSSH *host certificate* instead of
a plain key, OneTerm refuses it and says so as an ordinary connection error — no "trust this host
key?" prompt, because there would be nothing to trust it against and approving could never work. It
cannot happen with a well-behaved server, since OneTerm never asks for certificates, and a test
proves that even when the certificate wraps a key OneTerm already trusts, the connection is still
refused.

There is nothing left here for a user to notice, and nothing left for the owner to accept beyond
the two items the packet already lists.
