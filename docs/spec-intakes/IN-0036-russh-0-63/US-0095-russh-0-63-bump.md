# Work: Bump russh to 0.63 and russh-sftp to 3.0, off the release-candidate crypto stack

ID: US-0095
Intake: IN-0036
Created: 2026-09-15

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
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

- [ ] In scope: root `Cargo.toml` (two requirements), `Cargo.lock`, the 10 call sites and 1
  silent semantic site listed in
  [`low-level-design/upgrade.md`](low-level-design/upgrade.md), one new test module
  (`crates/ssh/src/keyfile_tests.rs`), regenerated `THIRD-PARTY-NOTICES.md`, and the doc
  reconciliation below.
- [ ] Out of scope: adopting anything russh 0.63 newly offers — host certificates
  (`Preferred::host_key_certificates`), GSSAPI (`authenticate_gssapi_with_mic`), and
  russh-sftp's `expand-path@openssh.com` are all left unused and un-advertised. Tuning the new
  `russh-sftp` concurrency knobs (defaults are taken). Declaring `russh-cryptovec` or
  `russh-util` directly — `docs/agents/dependencies.md` §3 forbids it and they stay transitive.
  `deny.toml` (no licence enters the graph that the allow-list does not already cover — proven
  in Evidence, not assumed).

## Acceptance

- [ ] `cargo tree -p oneterm-ssh` shows `russh 0.63.3` and `russh-sftp 3.0.0`; no `russh 0.61.x`
      or `russh-sftp 2.x` anywhere in the graph.
- [ ] `Cargo.lock` contains exactly three `-rc` crates, and each is its own crates.io `latest`.
- [ ] Host-key verification is not weakened: `check_server_key` refuses a host certificate
      explicitly and the five `verify_server_key` outcomes are byte-for-byte the previous logic.
- [ ] Forwarding defaults per `DEC-0011` are not weakened: an unrequested agent-forward or
      forwarded-tcpip channel is refused (now on the wire), and SOCKS5 / loopback-bind /
      per-session agent opt-in are untouched.
- [ ] Jump-host chaining per `DEC-0010` still works: `route_tests` two-hop relay passes, and the
      relay-refused case is still an explicit rejection.
- [ ] `cargo test -p oneterm-ssh -p oneterm-sftp-ui -p oneterm-tools` at or above baseline
      (68 / 49 / 16 — no shrink), plus the new key-file suite.
- [ ] `cargo deny check licenses bans advisories` exits 0 with **zero** `warning[yanked]`.
- [ ] `python scripts/third-party-notices.py --check` passes after regeneration; every licence
      row that changed is reported and checked against `deny.toml` / `docs/license-analysis.md`.
- [ ] `python scripts/verify-dependency-graph.py` passes.
- [ ] `pwsh scripts/ci-local.ps1 --full` green, workspace test totals at or above the `main`
      baseline (60 sections / 1976 passed / 0 failed / 14 ignored, 12 steps).

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

_To be completed before marking Implemented._

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

- [ ] Capture the baseline: `cargo tree -i` per `-rc` crate, test counts, `cargo deny` output.
- [ ] Bump the two requirements in root `Cargo.toml`; `cargo update -p russh -p russh-sftp`,
      then `cargo update -p argon2 -p blake2` (they only move to stable when asked; they are not
      pulled forward by the russh bump).
- [ ] Apply Changes A-D from the LLD, smallest diff per site.
- [ ] Add Change E (`crates/ssh/src/keyfile_tests.rs`).
- [ ] Regenerate `THIRD-PARTY-NOTICES.md`; diff the licence column, not just the versions.
- [ ] Update `docs/agents/dependencies.md` §3 and `docs/ssh-client-connect.md`.
- [ ] Run the gates below; record any behaviour that needs the owner's acceptance under
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
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

_To be completed after implementation._

## Handoff

None; implemented in the same session.
