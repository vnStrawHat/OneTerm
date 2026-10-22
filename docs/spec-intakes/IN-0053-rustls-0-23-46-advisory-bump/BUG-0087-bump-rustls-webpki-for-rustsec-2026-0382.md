# Work: Bump rustls-webpki to 0.103.16 for RUSTSEC-2026-0382

ID: BUG-0087
Intake: IN-0053
Created: 2026-10-22

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: maintenance (security advisory, lockfile-only patch bump)
- Risk lane: normal — `rustls-webpki` is reached only through `reqwest` / `hyper-rustls` / `tokio-rustls` in `oneterm-update` (update check and download over HTTPS). SSH uses `russh`, not rustls. Exposure: webpki would accept a certificate path through a distrusted emSign intermediate when the trust anchors allow it; the update client is the only TLS client, so the bump is for the update channel's trust decisions and for the `cargo deny` gate.
- Spec Intake, when required: IN-0053 (same lineage as IN-0034 / BUG-0085: a rustls-family advisory fixed lockfile-only)

## Outcome

`Cargo.lock` resolves `rustls-webpki` to >= 0.103.16 and `cargo deny check licenses bans advisories` passes; update-check / download behaviour is unchanged.

## Scope

- [x] In scope: `Cargo.lock`, `THIRD-PARTY-NOTICES.md` if the graph changes.
- [x] Out of scope: any manifest or source change; a `deny.toml` ignore.

## Acceptance

- [x] `cargo tree -i rustls-webpki` shows one version, >= 0.103.16.
- [x] `cargo deny check licenses bans advisories` passes.
- [x] `third-party-notices.py --check` passes; `oneterm-update` and `oneterm-settings-ui` tests pass.

## Documentation

### Owning Docs Reviewed

- `docs/agents/dependencies.md` — names `reqwest` for auto-update, no `rustls-webpki` version; the lockfile is the record.
- `docs/spec-intakes/IN-0053-rustls-0-23-46-advisory-bump/BUG-0085-lock-rustls-at-0-23-46-for-rustsec.md` — same change for rustls.

### Documentation Action

- No contract change: no doc names the `rustls-webpki` version; behaviour is unchanged.

Reason: lockfile-only patch release (0.103.15 -> 0.103.16, semver compatible).

### Reconciliation

Confirmed at completion (see Evidence).

## Context

Advisory RUSTSEC-2026-0382 "Certificate verification may accept distrusted intermediates": affected `rustls-webpki` < 0.103.16 (and 0.104 alphas before 0.104.0-alpha.8). Fix: >= 0.103.16.

## Plan

- [x] `cargo update -p rustls-webpki` (picks 0.103.16, nothing else moves per dry run); `cargo tree -i rustls-webpki`.
- [x] `cargo deny`, notices check, test the TLS users.

## Decisions

None.

## Verification Plan

`cargo deny check licenses bans advisories`; `cargo test -p oneterm-update`; `cargo test -p oneterm-settings-ui`; `python scripts/third-party-notices.py --check`; `pwsh scripts/ci-local.ps1`.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

- `cargo deny check advisories` on main failed: RUSTSEC-2026-0382 on `rustls-webpki 0.103.15` (via rustls 0.23.46 -> hyper-rustls / tokio-rustls / reqwest 0.12.28 -> `oneterm-update`).
- `cargo update -p rustls-webpki`: `0.103.15 -> 0.103.16`, nothing else moved; `Cargo.lock` 2 lines. `cargo tree -i rustls-webpki`: one version, 0.103.16. `THIRD-PARTY-NOTICES.md` regenerated (1 line), `--check` ok.
- `cargo deny check licenses bans advisories`: advisories ok, bans ok, licenses ok; no `deny.toml` change.
- `pwsh scripts/ci-local.ps1 --full` (CARGO_BUILD_JOBS=2): all checks passed, 141 suites / 5120 passed / 0 failed (includes `oneterm-update` and `oneterm-settings-ui`). The update tests use local HTTP, so no test exercises a real TLS handshake against GitHub; no real-host check was made.

## Handoff

None.
