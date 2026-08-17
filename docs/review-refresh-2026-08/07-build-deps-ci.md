# 07 — Build, dependencies, vendor forks, CI, scripts

> Part of the [2026-08 refresh review](README.md). Checklist format.

## Assessment

Above average for a project of this age: pinned action SHAs, a machine-readable dependency-graph policy
enforced in CI, a documented patch-set model for the three vendored forks with a reproducible
`vendor/refresh.sh`, edition-2024 workspace lints with `dbg!`/`todo!` denied, a single `VERSION` source
feeding rc/plist/About, and a release pipeline that publishes only after all targets build. All Python
gates pass. The gaps: identity/versioning (every crate is 0.0.0, toolchain floats), a CI blind spot on
the primary platform, vendor weight vs. delta (46 MB of unused alacritty test recordings; a 259-file
gpui-component fork carrying ~40 lines), manifest debt, and a build-time `git remote` lookup baked
into the updater.

---

## A. Cargo manifests & dependencies

- [x] **[High] BUILD-02 — No crate declares `version`; all 18 packages are 0.0.0** while `VERSION` says 0.3.9.
  `Cargo.toml` `[workspace.package]` (l.30-32) + every `crates/*/Cargo.toml`. `CARGO_PKG_VERSION`,
  `cargo tree`, crash reports and any future packaging see 0.0.0. *Fix:* `version = "0.3.9"` (+ `license`,
  `repository`, `rust-version`) in `[workspace.package]`, `version.workspace = true` in each crate;
  `scripts/bump-version.sh` rewrites both (or drop `VERSION` and use `CARGO_PKG_VERSION`).

- [x] **[Medium] BUILD-03 — Toolchain floats.** No `rust-toolchain.toml`, no `rust-version`; workflows use
  `dtolnay/rust-toolchain@…` "stable". A new stable clippy lint breaks `-D warnings` CI without any code
  change. *Fix:* `rust-toolchain.toml` (`channel = "1.xx"`), `rust-version` in `[workspace.package]`, CI reads it.

- [x] **[Medium] BUILD-04 — 10 declared dependencies with zero use.** `crates/agent-ui/Cargo.toml`
  (`oneterm-core`, `oneterm-theme`, `log`), `crates/session-ui/Cargo.toml` (`anyhow`),
  `crates/settings-ui/Cargo.toml` (`serde`, `serde_json`), `crates/sftp-ui/Cargo.toml` (`serde`, `serde_json`),
  `crates/workspace/Cargo.toml` (`oneterm-theme`, `serde`). `scripts/dependency-graph-policy.json` even
  encodes the unused `agent-ui→core/theme` and `workspace→theme` edges. *Fix:* remove; update the policy
  and `docs/agents/structure.md` §3; consider `cargo machete` in CI.

- [x] **[Medium] BUILD-05 — Unused workspace deps and a stale allowed-set doc.** `Cargo.toml` l.90 `smol = "2"`,
  l.117 `rust-i18n = "4"` used by no crate; `docs/agents/dependencies.md` §3 lists `tracing`,
  `tracing-subscriber`, `directories`, `toml`, `russh-cryptovec`, `ssh-key` — none exist. *Fix:* delete the
  two deps; rewrite §3.

- [x] **[Low] BUILD-06 — Direct deps not routed through `[workspace.dependencies]`.** `crates/app/Cargo.toml`
  l.71-80, `crates/core/Cargo.toml` l.19, `crates/local-shell/Cargo.toml` l.24-30 (`windows-sys 0.59`
  repeated 3× with diverging features; 0.61 is already in the graph), `crates/terminal-view/Cargo.toml` l.35
  (`itertools`), `crates/theme/Cargo.toml` l.18 (`rust-embed`), local-shell dev `libc`. *Fix:* centralise;
  unify `windows-sys = "0.61"`.

- [ ] **[Low] BUILD-07 — 47 duplicated crates in `Cargo.lock`** (windows-* 0.58/0.61/0.62, `sha2` 0.10/0.11,
  `rand_core` ×3, `bitflags` 1/2, `hashbrown` ×3, `thiserror` 1/2, `toml` 0.8/1.1, `itertools` ×3) — mostly
  gpui@1d217ee vs russh 0.61; nothing tracks it. *Fix:* `cargo deny check bans` (`multiple-versions = "warn"`);
  bump own `sha2` → 0.11, `windows-sys` → 0.61.

- [ ] **[Low] BUILD-08 — Release profile comment is false.** `[profile.release]` l.150-157: `debug = 0` +
  `strip = "symbols"` with "split debug info remains available" — no PDB/dSYM is generated, so release crash
  dumps cannot be symbolicated. *Fix:* `debug = "line-tables-only"` + `split-debuginfo = "packed"` and upload
  `.pdb`/`.dSYM` as a release artifact; or fix the comment.

- [ ] **[Low] BUILD-09 — Seven first-party crates forced to `opt-level = 3` in dev** (`[profile.dev.package]`
  l.116-142); `profile.test` inherits it. *Fix:* named `[profile.fast-dev]` (`inherits = "dev"`) or override
  only `alacritty_terminal`.

- [ ] **[Low] BUILD-10 — `[patch]` indirection for a fully vendored crate.** `alacritty_terminal = { git = … }`
  + `[patch."https://github.com/zed-industries/alacritty"]` (l.174) still clones upstream on a cold cache.
  *Fix:* `alacritty_terminal = { path = "vendor/alacritty_terminal" }` directly, documenting the base rev.

- [x] **[Low] BUILD-11 — `publish = false` repeated 18×; no `license` field anywhere.** *Fix:*
  `publish.workspace = true`, `license.workspace = true`.

## B. build.rs & scripts

- [x] **[Medium] BUILD-12 — Updater target repo inferred at build time from `git remote`.**
  `crates/update/build.rs:16-23` (`infer_git_remote`) — non-hermetic; a fork/mirror build ships an updater
  pointing at that fork; no `rerun-if-changed=.git/config`; `UPDATE_REPOSITORY` may be `""`; and
  `settings-ui/src/about.rs:26` hard-codes a second `GITHUB_REPOSITORY_URL`. *Fix:* hard-code the canonical
  repo constant in `crates/update/src/config.rs`, allow `ONETERM_UPDATE_REPO` env override only; derive the
  About URL from it.

- [x] **[Medium] BUILD-13 — `VERSION` is read by four build scripts.** `crates/app/build.rs:24-34` (twice),
  `crates/workspace/build.rs`, `crates/settings-ui/build.rs`, `crates/update/build.rs` (which silently falls
  back to `"0.0.0"` where the others panic). *Fix:* with BUILD-02, delete the three copies and use
  `env!("CARGO_PKG_VERSION")`; keep only `app/build.rs` (resources) and `theme/build.rs`.

- [ ] **[Low] BUILD-14 — ConPTY binaries copied for any Windows target; no third-party notice.**
  `crates/app/build.rs:85-97` copies x64 `conpty.dll`/`OpenConsole.exe` even for `aarch64-pc-windows-msvc`
  (advertised in README); the redistributed Windows Terminal binaries have no version, source URL, hash or
  MIT notice. *Fix:* gate on `CARGO_CFG_TARGET_ARCH == "x86_64"`; add `THIRD-PARTY-NOTICES.md`.

- [ ] **[Low] BUILD-15 — Release scripts copy developer state into `dist/`.** `scripts/build-release.sh:71-74`,
  `build-release.ps1:58-63` copy `terminal.json`/`docks.json` from the repo root if present (git-ignored
  personal files). *Fix:* remove.

- [ ] **[Low] BUILD-16 — Scripts not wired into CI / duplicated.** `completion-catalog.py validate`,
  `test_highlight.sh` not run; `.ps1`/`.sh` demo pairs are 300/240 lines of duplicated logic; no
  `scripts/README.md`. *Fix:* add `completion-catalog.py validate` to the dependency-graph job; document
  manual-only scripts.

- [ ] **[Low] BUILD-17 — `check-doc-paths.py` only validates `docs/architecture.md`.**
  `docs/agents/structure.md` (full tree, `update/` listed twice at l.94/l.131, refers to
  `docs/refactor/ui-crate-restructure.md` as "authoritative") is unchecked. *Fix:* extend to `docs/agents/*.md`
  and `README.md`.

## C. Vendor forks

- [x] **[High] BUILD-18 — 46 MB of unused alacritty test recordings are tracked.**
  `vendor/alacritty_terminal/tests/**` (≈190 files, 46 MB vs 438 KB of `src/`); the crate is `exclude`d so
  these tests never run; `vendor/vte/.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig`, `Cargo.lock` are
  also tracked. *Fix:* `VENDOR_PRUNE` list in `vendor/refresh.sh` (delete after fetch, before diff);
  regenerate; commit the removal.

- [x] **[Medium] BUILD-19 — Only one of three forks is drift-checked in CI.** `ci.yml` runs
  `check-ui-fork.py` (hash baseline of `vendor/gpui-component/src/`) only; nothing verifies `vendor/vte` or
  `vendor/alacritty_terminal` against pristine+patches, nor `vendor/gpui-component/{Cargo.toml,build.rs,locales}`
  (patch `0002` is outside the check surface). *Fix:* CI step `bash vendor/refresh.sh --check`; extend
  `check-ui-fork.py` to manifest and `build.rs`.

- [x] **[Medium] BUILD-20 — Rev-lock docs do not acknowledge the vendoring.** `docs/agents/dependencies.md`
  §1/§3 still say the alacritty dep is the Zed fork @ fcf32fe, never mention that it and `vte 0.15.0` are
  vendored/patched; `vendor/README.md` §6 shows `exclude` without gpui-component; `ui-fork-maintenance.md`
  says the check "rejects deltas outside `dock/tab_panel.rs`" (now three modules). *Fix:* add rows + a
  "vendored — see vendor/README.md" column; state the policy: "rev-lock = the pristine base rev; OneTerm
  deltas live exclusively in `vendor/patches/`".

- [ ] **[Medium] BUILD-21 — 259-file gpui-component fork carrying ~40 lines of delta.**
  `vendor/patches/gpui-component/0001` (45 lines) + `0003` (41 lines); the stated rationale in
  `ui-fork-maintenance.md` ("pub(crate) access across dock/resizable/tab/history") does not match the patch
  set. Every upstream bump re-snapshots 259 files. *Fix:* upstream both patches to longbridge/gpui-component;
  once merged, delete `vendor/gpui-component`, `check-ui-fork.py`, `ui-fork-baseline.json`; until then,
  correct the rationale.

- [ ] **[Low] BUILD-22 — `.gitattributes` covers only `vendor/gpui-component/**`** (`-whitespace`); no
  `eol=lf` for any vendor tree, so Windows `autocrlf` checkouts differ byte-wise from pristine. *Fix:*
  `vendor/** -whitespace text eol=lf`.

## D. CI & release

- [x] **[High] BUILD-01 — Windows (the primary platform) never runs clippy or the UI test suites.**
  `.github/workflows/ci.yml` `cross-platform-tests` (l.86-100): fmt/clippy/build/full `cargo test --workspace`
  run only on ubuntu; Windows/macOS run only `-p core -p terminal -p local-shell -p ssh`. The 111 tests in
  `terminal-view`, 57 in `completion`, and the ConPTY-specific `app`/`workspace` code are never compiled or
  tested on Windows in CI. *Fix:* run `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` on `windows-latest` (with `Swatinem/rust-cache`).

- [x] **[Medium] BUILD-23 — Workflow edits do not trigger CI.** `ci.yml` `on.push.paths` /
  `on.pull_request.paths` (l.6-27) omit `.github/workflows/**` (and `LICENSE`, `.gitattributes`).
  *Fix:* add `".github/**"` or drop path filtering.

- [ ] **[Medium] BUILD-24 — Release dispatch push can fail after the whole matrix built.**
  `release.yml` `release` job (l.343-360): checks out a SHA, commits `VERSION`, `git push origin HEAD:refs/heads/main`;
  if `main` advanced during the ~30-40 min build the push is rejected. `prepare` requests `contents: write`
  + `persist-credentials: true` it never uses. *Fix:* `git pull --rebase` before pushing, or make the workflow
  tag-driven only; reduce permissions.

- [x] **[Medium] BUILD-25 — No `cargo deny`/`cargo audit`, no `cargo doc`, no vendor `--check`, no catalog
  validation in CI.** `docs/license-analysis.md` relies on `zlog` (GPL-3.0-only, still in graph:
  `zlog ← ztracing ← sum_tree ← gpui`) being dead-stripped — nothing re-verifies. *Fix:* `cargo-deny` job with
  `deny.toml` (`[licenses]`, `[bans]`), plus the two Python checks.

- [ ] **[Low] BUILD-26 — Release build at the virtual-workspace root** (`release.yml` l.196
  `cargo build --release --no-default-features --features release-bin`) builds every member with default
  features off. *Fix:* `-p oneterm-app --no-default-features --features release-bin`.

- [ ] **[Low] BUILD-27 — Staging/packaging logic triplicated** (workflow l.199-235, `build-release.sh`,
  `build-release.ps1`); dist names differ (`oneterm-<ver>-<triple>` in CI vs `oneterm-<triple>` locally,
  README documents the local one); macOS bundle unsigned; no `SHA256SUMS`. *Fix:* workflow calls the scripts;
  unify name; publish `SHA256SUMS`; `codesign --force -s -` the .app.

- [ ] **[Low] BUILD-28 — No `concurrency:` group; redundant `cargo build --workspace` after
  `clippy --all-targets`.** *Fix:* `concurrency: {group: ci-${{ github.ref }}, cancel-in-progress: true}`;
  drop the build step.
