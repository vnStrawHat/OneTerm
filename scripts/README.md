# `scripts/` — OneTerm developer and CI scripts

Every script is runnable from the repository root. "CI" marks the scripts the
`.github/workflows/ci.yml` (`dependency-graph` and `vt-package` jobs) or `release.yml` pipelines run;
`scripts/ci-local.{sh,ps1}` runs the same CI set locally (`--full` / `-Full` adds the
network / extra-tool checks).

## Quality gate (run by CI and by `ci-local`)

| Script | Purpose | Runs in |
|---|---|---|
| `ci-local.sh` / `ci-local.ps1` | Run the whole CI quality gate locally, stop at the first failure. Keep both in sync with `ci.yml` and `AGENTS.md` §4. | local (mirrors CI) |
| `verify-dependency-graph.py` | Enforce `dependency-graph-policy.json` (workspace members, internal edges, backend/feature rules), that every crate inherits `[workspace.package] version`, that **no** crate is publishable (owner ruling 2026-09-15: nothing goes to crates.io), and, given a `cargo package --list` on stdin via `--package-list -`, that the `oneterm-vt` package carries `README.md`, `CHANGELOG.md`, `LICENSE`, `NOTICE` and `examples/headless.rs` and reaches nothing outside `crates/vt`. | CI |
| `vt-public-api.py` | Enumerate `oneterm-vt`'s public API from its rustdoc output and diff it against the committed `crates/vt/public-api.txt`, so a change to the public surface is a line in a review. `--write` regenerates, `--check` gates, `--no-doc` reuses an existing `target/doc`. | CI (`vt-package` job) |
| `check-doc-paths.py` | Every back-ticked `crates/`, `docs/`, `scripts/` path in the current-state docs (`docs/architecture.md`, `docs/agents/*.md`, `docs/README.md`, `README.md`, `AGENTS.md`) must exist. | CI |
| `check-english.py` (+ `test_check_english.py`) | English-only contributor text (code comments, docs); the unittest file tests the checker itself. | CI |
| `completion-catalog.py validate` | Validate the completion catalogs under `crates/completion/assets/` against the schema. Other subcommands (`download`, `generate`, `update`) fetch/parse upstream docs (network) — see `docs/auto-completion/07-external-assets-script.md`. `completion-commands.json` is its curated command whitelist. | CI (validate only) |
| `benchmark-scale.py` | Run the scale benchmarks (slow, manual); `--list` prints the manifest. | manual |
| `third-party-notices.py --check` | `THIRD-PARTY-NOTICES.md` matches the resolved graph in `Cargo.lock` (offline `cargo metadata`) **and** the bundled ConPTY binaries still hash to `crates/app/assets/conpty-manifest.json`. Without `--check` it rewrites the file — run it after any dependency change or a `bump-conpty.ps1` run. | CI |
| `cargo deny check licenses bans advisories` (`../deny.toml`) | Licence / duplicate / advisory policy (needs `cargo install cargo-deny`). | CI, `ci-local --full` |

## Release

| Script | Purpose | Runs in |
|---|---|---|
| `build-release.ps1` | Windows: build `oneterm.exe` (`-p oneterm-app`), stage `dist/oneterm-<version>-<triple>/` (+ `conpty.dll`, `x64/OpenConsole.exe`), zip + `.sha256`. | `release.yml`, local |
| `build-release.sh` | Linux / macOS twin (`TARGET=<triple>` for cross builds); on macOS calls `bundle-macos.sh`; tar.gz + `.sha256`. | `release.yml`, local |
| `bundle-macos.sh` | Assemble + ad-hoc-sign `OneTerm.app` from a built binary (Info.plist from `crates/app/assets/macos/`, best-effort `.icns`). | via `build-release.sh` |
| `bump-conpty.ps1` | Install a new version of the bundled Windows console host pair (`crates/app/assets/conpty.dll` + `x64/OpenConsole.exe`) from the `Microsoft.Windows.Console.ConPTY` NuGet package and record it in `crates/app/assets/conpty-manifest.json`. `-Latest` or `-Version <x.y.z>` to bump, `-Check` to verify the tracked files against the manifest offline. | manual |

Bumping the console host:

```powershell
pwsh scripts/bump-conpty.ps1 -Latest        # or -Version 1.24.260710001
python scripts/third-party-notices.py       # regenerate THIRD-PARTY-NOTICES.md §1 from the manifest
pwsh scripts/bump-conpty.ps1 -Check         # assets == manifest (also enforced by the notices --check in CI)
```

The script refuses binaries that are not validly Authenticode-signed by Microsoft. To go back to
a previous pair use git (`git checkout <rev> -- crates/app/assets`), not `-Version`: old package
versions are not guaranteed to stay on the NuGet flat container. Rationale and rules:
[`../docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md`](../docs/decisions/DEC-0013-bundled-conpty-host-and-bump-script.md).

## Manual test helpers (never run by CI)

| Script | Purpose |
|---|---|
| `osc-test.sh` / `osc-test.ps1` | Interactive OSC sequence tester — run **inside** a OneTerm terminal, pick one sequence at a time (`docs/osc-sequences-checklist.md`). |
| `agent-status-demo.sh` / `agent-status-demo.ps1` | Emit OSC 20308;1 agent-status events to exercise the Agent Panel (`docs/osc-agent-status.md`). |
| `test_highlight.sh` | Print sample output covering every semantic-highlight class (`docs/terminal-semantic-highlighting.md`); eyeball the colours. |

The `.sh` / `.ps1` pairs are intentionally duplicated (Git Bash is not guaranteed on
Windows dev machines, PowerShell is not guaranteed elsewhere); when you change one,
change the other. `bump-conpty.ps1` has no `.sh` twin on purpose: it installs Windows
binaries and verifies their Authenticode signature, so it can only run on Windows — the
cross-platform half of the job (assets vs manifest) lives in `third-party-notices.py`.

Developer diagnostics that need a Rust build live in `crates/tools`
(`cargo run -p oneterm-tools --release --bin doom-fire` / `--bin pty-throughput`).
