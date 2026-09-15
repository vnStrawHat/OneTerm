# Work: Bump russh to 0.63 and russh-sftp to 3.0, off the release-candidate crypto stack

ID: US-0095
Intake: IN-0036
Created: 2026-09-15

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

- Change type: maintenance (dependency upgrade)
- Risk lane: **high-risk** — authentication, host-key verification, secrets (private key files),
  and an external effect (every SSH/SFTP connection OneTerm makes).
- Spec Intake: IN-0036

## Outcome

`Cargo.toml` requires `russh = "0.63"` and `russh-sftp = "3.0"`; the workspace compiles, lints
and tests clean; and every auth, host-key, forwarding and SFTP guarantee OneTerm made before the
bump still holds, with each crossed behaviour change named and either preserved or explicitly
recorded.

The release-candidate crypto pins drop from **16** crates to **3**
(`ssh-key 0.7.0-rc.11`, `rsa 0.10.0-rc.18`, `pkcs1 0.8.0-rc.4`), each of which stays only
because no stable release of it exists on crates.io.

## Scope

- [x] In scope: root `Cargo.toml` (two requirements), `Cargo.lock`, the 10 call sites and 1
  silent semantic site listed in
  [`low-level-design/upgrade.md`](low-level-design/upgrade.md), one new test module
  (`crates/ssh/src/keyfile_tests.rs`), regenerated `THIRD-PARTY-NOTICES.md`, and the doc
  reconciliation below.
- [x] Out of scope: adopting anything russh 0.63 newly offers — host certificates
  (`Preferred::host_key_certificates`), GSSAPI (`authenticate_gssapi_with_mic`), and
  russh-sftp's `expand-path@openssh.com` are all left unused and un-advertised. Tuning the new
  `russh-sftp` concurrency knobs (defaults are taken). Declaring `russh-cryptovec` or
  `russh-util` directly — `docs/agents/dependencies.md` §3 forbids it and they stay transitive.
  `deny.toml` (no licence enters the graph that the allow-list does not already cover — proven
  in Evidence, not assumed).

## Acceptance

- [x] `cargo tree -p oneterm-ssh` shows `russh 0.63.3` and `russh-sftp 3.0.0`; no `russh 0.61.x`
      or `russh-sftp 2.x` anywhere in the graph.
- [x] `Cargo.lock` contains exactly three `-rc` crates, and each is its own crates.io `latest`.
- [x] Host-key verification is not weakened: `check_server_key` refuses a host certificate
      explicitly and the five `verify_server_key` outcomes are byte-for-byte the previous logic.
- [x] Forwarding defaults per `DEC-0011` are not weakened: an unrequested agent-forward or
      forwarded-tcpip channel is refused (now on the wire), and SOCKS5 / loopback-bind /
      per-session agent opt-in are untouched.
- [x] Jump-host chaining per `DEC-0010` still works: `route_tests` two-hop relay passes, and the
      relay-refused case is still an explicit rejection.
- [x] `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools` at or above baseline
      (68 / 49 / 16 — no shrink), plus the new key-file suite. Result: 73 / 49 / 16.
- [x] `cargo deny check licenses bans advisories` exits 0 with **zero** `warning[yanked]`.
- [x] `python scripts/third-party-notices.py --check` passes after regeneration; every licence
      row that changed is reported and checked against `deny.toml` / `docs/license-analysis.md`.
- [x] `python scripts/verify-dependency-graph.py` passes.
- [x] `pwsh scripts/ci-local.ps1 --full` green, workspace test totals at or above the `main`
      baseline (60 sections / 1976 passed / 0 failed / 14 ignored, 12 steps). Result:
      60 / 1981 / 0 / 14, 12 steps.

## Documentation

### Owning Docs Reviewed

- `docs/ssh-client-connect.md` — the connect / auth / host-key contract. Needs review against
  Change A: it is the doc that states what happens to an unrecognised host key.
- `docs/sftp-browser-design.md` — SFTP browser behaviour and the `SftpStatus` error mapping.
  Reviewed against russh-sftp 3.0's error and attribute changes.
- `docs/agents/dependencies.md` §3 — the "SSH and SFTP" row names `russh` (features `ring`,
  `flate2`, `rsa`) and `russh-sftp`. §3's "do not re-add" list names `russh-cryptovec` and
  `ssh-key`; this change keeps both transitive.
- `docs/agents/crate-dependency-rules.md` — R1-R12 on workspace declarations. Two existing
  requirements move; no crate is added to or removed from `[workspace.dependencies]`.
- `docs/license-analysis.md` + `deny.toml` — the licence allow-list. Five crates enter the graph
  (`num-bigint`, `sha3`, `sponge-cursor`, `syn 3`, `wnaf`); their licences must already be
  allowed or the analysis must be updated.
- `docs/decisions/DEC-0010-jump-hosts-reference-saved-sessions.md` and
  `docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md` — the contracts Change B
  and C must not weaken.
- `docs/spec-intakes/IN-0023-ssh-tunnels-jump-hosts-agent-auth/` — the intake that last moved
  this russh surface; it names the exact APIs (`client::connect`, `check_server_key`,
  `keys::known_hosts::*`, `AgentClient`, `PrivateKeyWithHashAlg`, `MethodSet`/`MethodKind`,
  `Channel`/`ChannelMsg`, direct-tcpip / forwarded-tcpip, SOCKS5, `SftpSession`).
- `docs/spec-intakes/IN-0035-yanked-russh-crypto-crates/high-level-design.md` — the rejected
  alternative that scoped this change.
- `docs/agents/error-policy.md` — how `SshHandlerError` / `AppError` surface; the certificate
  refusal must fit the existing shape rather than invent a variant.

### Documentation Action

Update required:

- `docs/agents/dependencies.md` §3 — the SSH row's version requirements move (`russh 0.61` ->
  `0.63`, `russh-sftp 2.3` -> `3.0`). This is exactly what the table is for.
- `docs/ssh-client-connect.md` — the host-key section must state that a host **certificate** is
  refused, and why (no CA trust store, no certificate algorithm advertised). This is new
  externally visible behaviour on the auth path, so the contract must say it.

Reviewed with no change expected (confirm in Reconciliation):

- `docs/sftp-browser-design.md` — the browser reads `SftpStatus`, which does not change.
- `docs/license-analysis.md` / `deny.toml` — only if a new licence appears.
- `DEC-0010`, `DEC-0011`, `IN-0023` — their contracts are preserved, not amended.

Reason: a dependency version table and an auth-path behaviour statement are both contracts this
change actually moves. Everything else describes OneTerm-side policy that the bump leaves alone.

### Reconciliation

Docs changed:

- `docs/agents/dependencies.md` §3 — the "SSH and SFTP" row now carries the concrete
  requirements (`russh 0.63.x`, `russh-sftp 3.0.x`), states that the family moves together, and
  names the three crates that stay on a release candidate with the reason.
- `docs/ssh-client-connect.md` §9.3 — a sixth row in the host-key verification table: an OpenSSH
  host **certificate** is refused with a `cert:`-prefixed fingerprint, why (no CA trust store,
  known_hosts records bare keys), and why it cannot arise against a conforming server.
- This packet, `IN-0036.md`, `high-level-design.md`, `low-level-design/upgrade.md`.

Generated artifact changed: `THIRD-PARTY-NOTICES.md` — 903 -> 905 packages; the full row diff is
in Evidence. **No licence entered or left the allow list.** Every added crate is
`MIT OR Apache-2.0` / `Apache-2.0 OR MIT`, both already in `deny.toml`'s `licenses.allow`.

Recorded no-change reasons, re-confirmed after implementation:

- `docs/license-analysis.md` — §1's distribution table is an explicitly approximate, dated
  snapshot (`~840 crate entries`); the net change is +2 crates, all inside the existing
  "Apache-2.0 OR MIT" group. §6 ("Crates Requiring Special Attention") lists `zlog`, `ztracing`,
  `ztracing_macro`, `dwrote`, `option-ext`, `libbz2-rs-sys`, `self_cell` — none of them moved.
  The one arguable change is in OneTerm's favour: `fiat-crypto`
  (`MIT OR Apache-2.0 OR BSD-1-Clause`, the graph's only BSD-1-Clause option) left with the
  released `curve25519-dalek`. Nothing entered that the analysis does not already cover, so no
  update is required.
- `deny.toml` — `licenses.allow`, the three `licenses.exceptions`, `advisories.ignore` and
  `bans.multiple-versions = "warn"` are all unchanged and all still correct. Duplicate warnings
  went 77 -> 79 (`generic-array` and `hybrid-array` now resolve to two versions each under
  russh 0.63); `multiple-versions` is `warn` by policy, so this is reported, not failed.
- `docs/sftp-browser-design.md` — describes the two-channel architecture and the
  `SftpSession::new(channel.into_stream())` call, neither of which changed. It pins no
  russh-sftp version and states no concurrency budget, so the raised in-flight defaults do not
  contradict it.
- `docs/agents/crate-dependency-rules.md` — R1-R12 govern workspace declarations. Two existing
  requirements moved; nothing was added to or removed from `[workspace.dependencies]`, and
  `russh-cryptovec` / `russh-util` stayed transitive as `dependencies.md` §3's "do not re-add"
  list requires. `scripts/verify-dependency-graph.py` passes.
- `DEC-0010`, `DEC-0011`, `IN-0023` — their contracts are preserved. `DEC-0011` is in fact
  enforced more strictly than before (see Evidence, behaviour change 2), which is a
  strengthening of the accepted decision rather than a change to it, so the decision text
  stands.

## Context

Established by compiling the bumped workspace, not by guessing: **10 compile errors across 6
files**, two distinct API changes, plus one compiler-silent semantic change. Full before/after
in [`low-level-design/upgrade.md`](low-level-design/upgrade.md); the changelog walk, the
crypto-graph before/after and the risk table are in
[`high-level-design.md`](high-level-design.md).

The two reassurances that keep the blast radius small, both established by diffing the vendored
sources rather than by trusting release notes:

- `russh/src/keys/known_hosts.rs` and `russh/src/keys/agent/client.rs` are **byte-identical**
  between 0.61.2 and 0.63.3.
- The kex, cipher, MAC and compression preference lists — and `Preferred::DEFAULT.key` — are
  **byte-identical**. No wire negotiation changes; RSA-SHA2 behaviour is unchanged.

## Plan

- [x] Capture the baseline: `cargo tree -i` per `-rc` crate, test counts, `cargo deny` output.
- [x] Bump the two requirements in root `Cargo.toml`; `cargo update -p russh -p russh-sftp`,
      then `cargo update -p argon2 -p blake2` (they only move to stable when asked; they are not
      pulled forward by the russh bump).
- [x] Apply Changes A-D from the LLD, smallest diff per site.
- [x] Add Change E (`crates/ssh/src/keyfile_tests.rs`).
- [x] Regenerate `THIRD-PARTY-NOTICES.md`; diff the licence column, not just the versions.
- [x] Update `docs/agents/dependencies.md` §3 and `docs/ssh-client-connect.md`.
- [x] Run the gates below; record any behaviour that needs the owner's acceptance under
      "Owner decision required".

## Decisions

None recorded. The two choices this change makes — refuse host certificates, and take
russh-sftp's new concurrency defaults — are both "keep doing what OneTerm already did", not
choices future work must inherit. If OneTerm ever wants host-certificate support, that is a new
capability with its own intake and its own decision record.

## Verification Plan

- Focused: `cargo tree -p oneterm-ssh`; the `-rc` crate census from `Cargo.lock`;
  `cargo info` per surviving `-rc` crate to prove no stable release exists.
- Unit + Integration: `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools`. The
  `crates/ssh/src/*_tests.rs` suites are in-process loopback integration tests, not mocks: they
  stand up a real `russh::server` on `127.0.0.1`, run a real key exchange, real publickey /
  password / keyboard-interactive auth, a real `russh::keys::agent::server`, real direct-tcpip
  and forwarded-tcpip channels, and a real two-hop jump-host chain. They are the proof.
- Platform / Release: `pwsh scripts/ci-local.ps1 --full` (12 steps),
  `cargo deny check licenses bans advisories`, `python scripts/third-party-notices.py --check`,
  `python scripts/verify-dependency-graph.py`.
- E2E: **not available.** No real SSH host and no GUI run in this environment. The loopback
  `sftp-dev-server` is run as a child process against the new russh-sftp as the closest
  available substitute, and its outcome recorded.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [ ] E2E proof
- [x] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Verify command: `pwsh scripts/ci-local.ps1 --full`.

### PROOF

```text
# 1. BASELINE, on main @ 621e4c9 (before any change)
$ cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools
oneterm-ssh     : 68 passed; 0 failed; 0 ignored
oneterm-sftp-ui : 49 passed; 0 failed; 0 ignored
oneterm-tools   : 14 + 2 passed; 0 failed; 0 ignored
Cargo.lock: 16 crates on a -rc version
  aes-gcm argon2 blake2 curve25519-dalek ecdsa ed25519-dalek elliptic-curve
  p256 p384 p521 pkcs1 primeorder rsa ssh-cipher ssh-encoding ssh-key
$ cargo deny check licenses bans advisories
advisories ok, bans ok, licenses ok    (0 yanked, 77 duplicate, 3 exception-not-encountered)

# 2. WHAT IS AVAILABLE UPSTREAM  (cargo info, 2026-09-15)
russh          0.61.2        -> latest 0.63.3
russh-sftp     2.3.0         -> latest 3.0.0
ssh-key        0.7.0-rc.10   -> latest 0.7.0-rc.11    <-- the rc IS the latest
rsa            0.10.0-rc.18  -> latest 0.10.0-rc.18   <-- the rc IS the latest
pkcs1          0.8.0-rc.4    -> latest 0.8.0-rc.4     <-- the rc IS the latest
ssh-cipher 0.3.0-rc.9 -> 0.3.0 | ecdsa 0.17.0-rc.18 -> 0.17.0
ed25519-dalek 3.0.0-rc.0 -> 3.0.0 | argon2 0.6.0-rc.8 -> 0.6.0 | blake2 0.11.0-rc.6 -> 0.11.0

# 3. THE BUMP
Cargo.toml: russh "0.61" -> "0.63" (features unchanged), russh-sftp "2.3" -> "3.0"
$ cargo update -p russh -p russh-sftp         -> Locking 38 packages
$ cargo update -p argon2 -p blake2            -> 2 more (they do not follow on their own)

# 4. AFTER: 16 -rc crates -> 3
$ grep -B2 'version = ".*-rc' Cargo.lock | grep 'name ='
name = "pkcs1"   (0.8.0-rc.4)
name = "rsa"     (0.10.0-rc.18)
name = "ssh-key" (0.7.0-rc.11)
   Each is its own crates.io `latest`; no stable release exists to move to.

$ cargo tree -i russh        -> russh v0.63.3        -> oneterm-ssh, oneterm-tools
$ cargo tree -i russh-sftp   -> russh-sftp v3.0.0    -> oneterm-ssh, oneterm-tools
$ cargo tree -i ssh-key      -> ssh-key v0.7.0-rc.11 -> russh v0.63.3 (sole path)
   No russh 0.61.x and no russh-sftp 2.x anywhere in the graph.

# 5. BEHAVIOUR CHANGES CROSSED  (each verified by diffing the vendored sources)
(1) check_server_key(&PublicKey) -> check_server_key(&PublicKeyOrCertificate).
    russh 0.63 supports OpenSSH host certificates; a certificate REPLACES the key
    check (russh client/mod.rs:1893). OneTerm refuses the Certificate arm with
    UnknownHostKey{fingerprint:"cert:<sha256>"}. Unreachable against a conforming
    server: Preferred::DEFAULT.host_key_certificates is Cow::Borrowed(&[]).
(2) Server-initiated channel opens are no longer pre-confirmed. 0.61 called
    confirm() BEFORE the handler; 0.63 hands the handler a ChannelOpenHandle and
    Drop sends AdministrativelyProhibited. OneTerm's two "never asked for this"
    paths now REFUSE on the wire where they previously confirmed-then-closed.
    Wire-visible; strengthens DEC-0011. Two tests re-asserted accordingly.
(3) Server-side Handler::channel_open_* lost its bool return (Result<bool,E> ->
    Result<(),E>) and gained the handle. 6 in-process servers moved.
(4) MethodSet::all() renamed to client_supported(); MethodKind gained
    GssapiWithMic. OneTerm uses only contains/empty/from -- no effect.
(5) russh-sftp: FileAttributes::default() flipped from the dummy attributes to
    empty(); the old value moved to dummy(). COMPILER-SILENT. One site
    (sftp-dev-server opendir fallback) changed to dummy().
(6) russh-sftp concurrency defaults: max_concurrent_writes 8 -> 16, new
    max_concurrent_reads 16 and max_write_packet_len 32 KiB. Throughput only.
(7) russh-sftp io::ErrorKind::TimedOut now maps to Error::Timeout instead of
    Error::IO. map_sftp_err's catch-all already handles it; message text only.

# 6. WHAT DID NOT CHANGE  (the reassurances, by diff)
$ diff russh-0.61.2/src/keys/known_hosts.rs  russh-0.63.3/src/keys/known_hosts.rs
  Files are identical.        <-- known_hosts format and round trip untouched
$ diff russh-0.61.2/src/keys/agent/client.rs russh-0.63.3/src/keys/agent/client.rs
  Files are identical.        <-- agent protocol and signing flags untouched
$ diff <(sed -n '105,150p' 0.61.2/src/negotiation.rs) <(sed -n '162,207p' 0.63.3/...)
  (empty)                     <-- SAFE_KEX_ORDER, CIPHER_ORDER, SAFE_HMAC_ORDER
                                  and COMPRESSION_ORDER byte-identical
Preferred::DEFAULT.key: same 7 entries, same order (Ed25519, ECDSA P256/P384/P521,
  RSA-SHA512, RSA-SHA256, ssh-rsa) -> preferred_key_algorithms() unchanged,
  RSA-SHA2 negotiation unchanged.
russh_sftp::protocol::VERSION == 3 in both releases -> SFTP dialect unchanged.

# 7. CODE CHANGED  (10 compile-error sites + 1 silent, exactly as the LLD listed)
crates/ssh/src/handler.rs        check_server_key, server_channel_open_agent_forward,
                                 server_channel_open_forwarded_tcpip
crates/ssh/src/agent_tests.rs    channel_open_session  (+ the refusal test, see 8)
crates/ssh/src/route_tests.rs    channel_open_session, channel_open_direct_tcpip
crates/ssh/src/session.rs        channel_open_session (EnvRecordingServer)
crates/ssh/src/task_tests.rs     channel_open_session
crates/ssh/src/tunnel_tests.rs   channel_open_direct_tcpip (+ the refusal test)
crates/tools/.../sftp-dev-server.rs  channel_open_session, FileAttributes::dummy()
crates/ssh/src/keyfile_tests.rs  NEW -- 5 tests, see 9
crates/sftp-ui                   UNTOUCHED (it names no russh symbol)

# 8. THE TWO TESTS THAT HAD TO CHANGE, AND WHY
agent_tests::without_the_switch_a_server_opened_agent_channel_is_closed_unanswered
tunnel_tests::a_remote_forward_reaches_the_local_target_and_unknown_ports_are_dropped
  Both asserted "the open succeeds, then EOF/Close arrives" -- the only signal
  available when russh confirmed before the handler ran. Under 0.63 the open
  itself now fails, so both assert the rejection instead. This is the change
  landing correctly, not a test relaxed to pass: each still proves the channel
  was not bridged, and now also proves the peer was told so.

# 9. NEW COVERAGE (crates/ssh/src/keyfile_tests.rs, 5 tests)
load_private_key was the only path reading a key from disk and had NO direct
test; the decode stack under it (ssh-key/ecdsa/ed25519-dalek/rsa/pkcs1/der) is
exactly what this bump moved.
  every_supported_key_algorithm_round_trips_through_a_file  (Ed25519, P-256/384/521)
  an_openssh_written_rsa_key_loads         (real ssh-keygen output -> interop)
  an_encrypted_key_loads_with_its_passphrase
  an_encrypted_key_rejects_the_wrong_passphrase
  an_encrypted_key_without_a_passphrase_is_refused

# 10. GATES (2026-09-15)
$ cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools
oneterm-ssh     : 73 passed; 0 failed; 0 ignored   (68 baseline + 5 new; no shrink)
oneterm-sftp-ui : 49 passed; 0 failed; 0 ignored   (== baseline)
oneterm-tools   : 14 + 2 passed; 0 failed          (== baseline)

$ cargo deny check licenses bans advisories
advisories ok, bans ok, licenses ok                (exit 0)
  warning[yanked]                            x 0   <-- acceptance criterion
  warning[duplicate]                         x 79  (was 77; bans = "warn")
  warning[license-exception-not-encountered]  x 3  (pre-existing)

$ python scripts/third-party-notices.py && python scripts/third-party-notices.py --check
wrote THIRD-PARTY-NOTICES.md / THIRD-PARTY-NOTICES.md is up to date.
  903 -> 905 packages.
  rc -> stable : aes-gcm 0.11.0-rc.4->0.11.1, argon2 0.6.0-rc.8->0.6.0,
    blake2 0.11.0-rc.6->0.11.0, curve25519-dalek 5.0.0-rc.0->5.0.0,
    ecdsa 0.17.0-rc.18->0.17.0, ed25519-dalek 3.0.0-rc.0->3.0.0,
    elliptic-curve 0.14.0-rc.33->0.14.1, p256/p384/p521 0.14.0-rc.10->0.14.0,
    primeorder 0.14.0-rc.10->0.14.0, ssh-cipher 0.3.0-rc.9->0.3.0,
    ssh-encoding 0.3.0-rc.9->0.3.0
  family      : russh 0.61.2->0.63.3, russh-sftp 2.3.0->3.0.0,
                russh-cryptovec 0.61.0->0.62.0, ssh-key 0.7.0-rc.10->0.7.0-rc.11
  added       : num-bigint 0.5.1, sha3 0.12.0, sponge-cursor 0.1.0, syn 3.0.5,
                wnaf 0.14.1                     (all MIT OR Apache-2.0)
  removed     : internal-russh-num-bigint 0.5.0, windows-registry 0.5.3,
                fiat-crypto 0.3.0
  incidental  : bitflags, bytes, hybrid-array, log, rfc6979, serde*, thiserror*,
                tokio-util, wasm-bindgen*
  LICENCE COLUMN: no licence group entered or left the graph. fiat-crypto's
  BSD-1-Clause option -- the graph's only one -- LEFT. deny.toml untouched.

$ python scripts/verify-dependency-graph.py
Dependency graph policy passed for 21 workspace packages and 21 explicit members.

$ pwsh scripts/ci-local.ps1 --full
ci-local: all checks passed.                              (exit 0)
  60 sections / 1981 passed / 0 failed / 14 ignored / 12 steps
  baseline was 60 / 1976 / 0 / 14 / 12 -- exactly +5, the new key-file tests.

# 11. LOOPBACK DEV SERVER against the new russh (real OpenSSH client)
$ target/debug/sftp-dev-server.exe --port 22987      (child process, killed by pid)
$ ssh -v -p 22987 -o BatchMode=yes dev@127.0.0.1
  remote software version russh_0.63.3
  kex: algorithm: mlkem768x25519-sha256
  kex: host key algorithm: ssh-ed25519
  kex: cipher (both directions): chacha20-poly1305@openssh.com  MAC: <implicit>
  Server host key: ssh-ed25519 SHA256:A2II+f5pWG6tYusUReLONkUcqAjWyyYoSN82W6VSlE0
  SSH2_MSG_NEWKEYS sent / SSH2_MSG_NEWKEYS received
  Authentications that can continue: password,publickey,hostbased,keyboard-interactive
  -> a full key exchange with a REAL OpenSSH client against the russh 0.63.3
     server. Auth then stops, because BatchMode cannot supply the password the
     dev server wants and Windows OpenSSH ignores a shell-script SSH_ASKPASS.
     The SFTP data path is therefore proven by the in-process suites, not here.
     Note the advertised method list carries NO gssapi-with-mic.
$ taskkill /PID 20396 /F   -> SUCCESS; no sftp-dev-server.exe left running.
```

### Owner decision required

Nothing blocks completion, but two items are the owner's to accept or redirect:

1. **`ssh-key`, `rsa` and `pkcs1` stay on release candidates.** Every other `-rc` crate is gone
   (16 -> 3). These three have no stable release on crates.io at all, so leaving them is the only
   option short of vendoring or patching, which would be worse. If the owner wants zero `-rc`
   crates, that means waiting for RustCrypto, not more work here.
2. **SFTP transfers now run at twice the previous in-flight write budget** (russh-sftp 3.0 raised
   `max_concurrent_writes` 8 -> 16 and added pipelined reads). The defaults were taken rather
   than pinned back to 8. This is a throughput change against real servers that this environment
   cannot measure; if a strict or slow server misbehaves, the knob is the `SftpSession` config
   at `crates/ssh/src/session.rs:529`.

### Gaps

- **E2E is unavailable and is not claimed.** There is no real SSH host and no GUI run in this
  environment. Everything above is loopback or in-process. Specifically unproven against a real
  server: interoperability of the raised SFTP concurrency budget, agent forwarding to a real
  `ssh-agent`, and a real jump-host chain.
- **The host-certificate refusal path (Change A's `Certificate` arm) has no test.** Forcing a
  server to present one would mean advertising `host_key_certificates` — that is, building the
  feature this packet explicitly put out of scope. The arm is a `return Err` with no branching,
  it is unreachable with `Config::default()`, and the LLD records it. If OneTerm ever adds
  certificate support, that intake owns the test.
- **The SFTP data path over `sftp-dev-server` was not driven by an external client.** Windows
  OpenSSH ignores a shell-script `SSH_ASKPASS`, so the batch `sftp` session could not
  authenticate. The key exchange against the real OpenSSH client did succeed (PROOF 11), and the
  SFTP data path is covered by the in-process `sftp_task` and `oneterm-sftp-ui` suites.
- **`bans` duplicate warnings rose 77 -> 79** (`generic-array` and `hybrid-array` each resolve to
  two versions under russh 0.63). `multiple-versions = "warn"` is deliberate policy, so this is
  reported rather than fixed; no crate OneTerm declares is duplicated.

### Harness rows

No `harness` tool is available in this worktree, so the `harness.db` rows are recorded here for
whoever has one. **Do not run this against a database that already holds these rows.**

```python
import sqlite3

db = sqlite3.connect("harness.db")
db.execute(
    "INSERT INTO intake (created_at, input_type, summary, risk_lane, risk_flags,"
    " affected_docs, story_id, doc_path, notes, document_number, design_doc)"
    " VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    (
        "2026-09-15",
        "maintenance",
        "Bump the russh family to 0.63 / russh-sftp 3.0 so the SSH crypto stack leaves the"
        " RustCrypto release candidates.",
        "high-risk",
        "auth,host-keys,secrets,external-effect,provider-sdk",
        "docs/ssh-client-connect.md,docs/sftp-browser-design.md,docs/agents/dependencies.md,"
        "docs/license-analysis.md,deny.toml,"
        "docs/decisions/DEC-0010-jump-hosts-reference-saved-sessions.md,"
        "docs/decisions/DEC-0011-forwarding-defaults-loopback-and-opt-in.md",
        "US-0095",
        "docs/spec-intakes/IN-0036-russh-0-63/IN-0036.md",
        "16 release-candidate crates drop to 3; ssh-key, rsa and pkcs1 stay because no stable"
        " release of them exists. Two security-relevant API moves: check_server_key takes a"
        " PublicKeyOrCertificate (host certificates refused), and server-initiated channel"
        " opens are no longer pre-confirmed (unrequested channels now refused on the wire).",
        36,
        "docs/spec-intakes/IN-0036-russh-0-63/high-level-design.md",
    ),
)
intake_id = db.execute("SELECT last_insert_rowid()").fetchone()[0]
db.execute(
    "INSERT INTO story (id, title, created_at, risk_lane, contract_doc, packet_doc, status,"
    " unit_proof, integration_proof, e2e_proof, platform_proof, evidence, verify_command,"
    " last_verified_at, last_verified_result, notes, intake_id)"
    " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (
        "US-0095",
        "Bump russh to 0.63 and russh-sftp to 3.0, off the release-candidate crypto stack",
        "2026-09-15",
        "high-risk",
        "docs/ssh-client-connect.md",
        "docs/spec-intakes/IN-0036-russh-0-63/US-0095-russh-0-63-bump.md",
        "implemented",
        1,
        1,
        0,
        1,
        "ci-local --full: 60 sections / 1981 passed / 0 failed / 14 ignored / 12 steps"
        " (baseline 1976; +5 new key-file tests). oneterm-ssh 73, oneterm-sftp-ui 49,"
        " oneterm-tools 16. cargo deny: advisories ok, bans ok, licenses ok, 0 yanked."
        " -rc crates 16 -> 3. No E2E: no real SSH host and no GUI in this environment.",
        "pwsh scripts/ci-local.ps1 --full",
        "2026-09-15",
        "passed",
        "Owner acceptance wanted on two items: the three surviving -rc crates (no stable"
        " release upstream), and russh-sftp 3.0's raised SFTP in-flight write budget (8 -> 16),"
        " taken as the default and unmeasured against a real server.",
        intake_id,
    ),
)
db.commit()
```

## Handoff

None; implemented in the same session.
