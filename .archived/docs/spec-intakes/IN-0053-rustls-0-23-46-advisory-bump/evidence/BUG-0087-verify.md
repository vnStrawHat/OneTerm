# Advisor review: BUG-0087 (rustls-webpki 0.103.15 -> 0.103.16, RUSTSEC-2026-0382)

Reviewed: 2026-10-22, head `fa8bca01` (2 commits on main `7ca8b515`), branch `verify/bug-0087`.
Env: `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

## Verdict: PASS

## Checks

1. Diff is lockfile-only. `git diff 7ca8b515..fa8bca01 -- Cargo.lock`: 2 lines, the `rustls-webpki` `version` and `checksum`; no other package moved. `THIRD-PARTY-NOTICES.md`: 1 line (the same version). No manifest or source change.
2. `cargo tree -q -i rustls-webpki`: one version, 0.103.16, reached only via `rustls 0.23.46 -> hyper-rustls 0.27.9 / tokio-rustls 0.26.4 -> reqwest 0.12.28 -> oneterm-update` (-> `oneterm-app`, `oneterm-settings-ui`).
3. `cargo deny check licenses bans advisories`: `advisories ok, bans ok, licenses ok`, 0 errors. 83 warnings, all pre-existing (identical count on main `7ca8b515`): 3 `license-exception-not-encountered` (`zlog`, `ztracing`, `ztracing_macro` in `deny.toml`) and 80 `duplicate` crate warnings; none names `rustls-webpki`. On main the same command fails with `error[vulnerability]: Certificate verification may accept distrusted intermediates`, so the bump is what clears the gate.
4. Advisory facts (https://rustsec.org/advisories/RUSTSEC-2026-0382.html, issued 2026-10-20, CVE-2026-111943, category crypto-failure): patched `>=0.103.16, <0.104.0-alpha.1` and `>=0.104.0-alpha.8`; affected versions may accept paths through distrusted emSign intermediates whose private keys are public, only when the trust anchors allow such paths. The packet's range, fix version and wording match.
5. Exposure: `Cargo.toml:116` builds `reqwest` with `rustls-tls-native-roots`, so the OS trust store is the anchor set and the advisory's "when the trust anchors allow it" caveat applies to the update channel (`crates/update/src/github.rs:92` `Client::builder()`); 0.103.16 rejects the keys regardless of the store. SSH: `cargo tree -i rustls` lists no `russh` path, so SSH is not affected. `docs/agents/dependencies.md:85` names `reqwest` for auto-update only.
6. `cargo test -p oneterm-update`: 60 passed, 0 failed. `python scripts/third-party-notices.py --check`: up to date. `check-english.py` and `check-doc-paths.py` pass.
7. Records: `BUG-0087` is free on main (highest is `BUG-0086`); IN-0053 is the right owner (rustls-family advisories, same shape as BUG-0085); the packet lists it.

## Findings

1. Low — `IN-0053.md:32`: the packet checklist row is `[ ] BUG-0087` while the packet's status is Implemented (BUG-0085/0086 rows are `[x]`). Tick it on merge.

## Owner still checks by hand

- No test exercises a real TLS handshake against GitHub (update tests use local HTTP); one manual update check after merge confirms the channel still negotiates.
