# `scripts/` — OneTerm developer and CI scripts

Every script is runnable from the repository root. "CI" marks the scripts the
`.github/workflows/ci.yml` (`dependency-graph` job) or `release.yml` pipelines run;
`scripts/ci-local.{sh,ps1}` runs the same CI set locally (`--full` / `-Full` adds the
network / extra-tool checks).

## Quality gate (run by CI and by `ci-local`)

| Script | Purpose | Runs in |
|---|---|---|
| `ci-local.sh` / `ci-local.ps1` | Run the whole CI quality gate locally, stop at the first failure. Keep both in sync with `ci.yml` and `AGENTS.md` §4. | local (mirrors CI) |
| `verify-dependency-graph.py` | Enforce `dependency-graph-policy.json` (workspace members, internal edges, backend/feature rules) and that every crate inherits `[workspace.package] version`. | CI |
| `check-ui-fork.py` (+ `ui-fork-baseline.json`) | Hash-pin the vendored `gpui-component` package against the reviewed baseline; `--update` re-clones upstream (network) and refreshes the baseline. | CI |
| `check-doc-paths.py` | Every back-ticked `crates/`, `docs/`, `scripts/`, `vendor/` path in the current-state docs (`docs/architecture.md`, `docs/agents/*.md`, `docs/README.md`, `README.md`, `AGENTS.md`) must exist. | CI |
| `check-english.py` (+ `test_check_english.py`) | English-only contributor text (code comments, docs); the unittest file tests the checker itself. | CI |
| `completion-catalog.py validate` | Validate the completion catalogs under `crates/completion/assets/` against the schema. Other subcommands (`download`, `generate`, `update`) fetch/parse upstream docs (network) — see `docs/auto-completion/07-external-assets-script.md`. `completion-commands.json` is its curated command whitelist. | CI (validate only) |
| `benchmark-scale.py --list` | Print the scale-benchmark manifest (validates it). Without `--list` it runs the benchmarks (slow, manual). | CI (`--list` only) |
| `third-party-notices.py --check` | `THIRD-PARTY-NOTICES.md` matches the resolved graph in `Cargo.lock` (offline `cargo metadata`). Without `--check` it rewrites the file — run it after any dependency change. | CI |
| `../vendor/refresh.sh --check` | Vendored forks == pristine upstream + `vendor/patches/` (network). | CI, `ci-local --full` |
| `cargo deny check licenses bans advisories` (`../deny.toml`) | Licence / duplicate / advisory policy (needs `cargo install cargo-deny`). | CI, `ci-local --full` |

## Release

| Script | Purpose | Runs in |
|---|---|---|
| `build-release.ps1` | Windows: build `oneterm.exe` (`-p oneterm-app`), stage `dist/oneterm-<version>-<triple>/` (+ `conpty.dll`, `x64/OpenConsole.exe`), zip + `.sha256`. | `release.yml`, local |
| `build-release.sh` | Linux / macOS twin (`TARGET=<triple>` for cross builds); on macOS calls `bundle-macos.sh`; tar.gz + `.sha256`. | `release.yml`, local |
| `bundle-macos.sh` | Assemble + ad-hoc-sign `OneTerm.app` from a built binary (Info.plist from `crates/app/assets/macos/`, best-effort `.icns`). | via `build-release.sh` |

## Manual test helpers (never run by CI)

| Script | Purpose |
|---|---|
| `osc-test.sh` / `osc-test.ps1` | Interactive OSC sequence tester — run **inside** a OneTerm terminal, pick one sequence at a time (`docs/osc-sequences-checklist.md`). |
| `agent-status-demo.sh` / `agent-status-demo.ps1` | Emit OSC 9;7 agent-status events to exercise the Agent Panel (`docs/osc-agent-status.md`). |
| `test_highlight.sh` | Print sample output covering every semantic-highlight class (`docs/terminal-semantic-highlighting.md`); eyeball the colours. |

The `.sh` / `.ps1` pairs are intentionally duplicated (Git Bash is not guaranteed on
Windows dev machines, PowerShell is not guaranteed elsewhere); when you change one,
change the other.

Developer diagnostics that need a Rust build live in `crates/tools`
(`cargo run -p oneterm-tools --release --bin doom-fire` / `--bin pty-throughput`).
