# AGENTS.md — OneTerm

Guide for AI agents (and contributors) when working with the **OneTerm** project — an SSH / SFTP / LocalShell GUI client


## 1. 📚 **Required reading** before writing code:
- `AGENTS.md` (this file) — to understand the overview, workflow, git, and quality gate.
- [`docs/agents/code-style.md`](docs/agents/code-style.md) — to learn the code conventions. These files are read by every agent session. Keep them high-signal.
- [`docs/agents/structure.md`](docs/agents/structure.md) — to learn the crate structure, directory tree, and dependency graph; and [`docs/agents/crate-dependency-rules.md`](docs/agents/crate-dependency-rules.md) for the **hard crate & dependency rules (R1–R12)**.
- [`docs/agents/error-policy.md`](docs/agents/error-policy.md) — to apply consistent runtime error handling and recovery rules.
- [`docs/agents/persistence.md`](docs/agents/persistence.md) — before changing persisted schemas or storage mechanics.
- [`docs/agents/dependencies.md`](docs/agents/dependencies.md) — to learn the rev lock & reference-first research.

---

## 2. Project introduction

**OneTerm** is a Terminal application for SSH/SFTP/Local Shell with a **Zed-style workspace UI**: 
- Connect to remote shells over SSH
- Browse and transfer files over SFTP
- Open local shells
- Monitor coding agents in a live Agent Panel fed by the OSC 9;7 proposal ([spec](docs/osc-agent-status.md)). 
- Powered by `Rust`, `alacritty_terminal`, `gpui`, `gpui-component`

---

## 3. Development guide

### 3.1 **DO NOT RUN**:

- `cargo init` / `cargo new` (the project already exists).
- `git clone` outside the workspace.
- Any command that modifies files outside project directory.
- `rm -rf` without a guard path.
- **Rootless / disk-wide searches** — `find /`, `grep -r /`, `rg` from the FS root, or any tool called without an explicit scope. **ALWAYS pass a concrete directory path** (e.g. `find crates/terminal-view/src -name '*.rs'`, `grep -rn 'foo' crates/`). This is a hard rule — see Core principle 7.


### 3.2. Learning the gpui-component API — read the reference first

Before writing UI code, use `find` / `grep` / `read` on `.\reference\gpui-component\` to look up APIs. See [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md) for the detailed lookup table. **Do not** use `web_search` for gpui-component unless the reference is missing something.

### 3.3. Common commands

⚠️ **Safety boundary**: Every command below **must only be run inside** project directory. Do not `cd` outside it, and do not run `cargo init` / `git clone` in any other directory.

```bash
# Format
cargo fmt --all

# Lint (must be warning-free)
cargo clippy --workspace --all-targets -- -D warnings

# Build
cargo build --workspace

# Release build
cargo build --workspace --release

# Run the app (dev binary = oneterm-debug, keeps the console for logs)
cargo run -p oneterm-app
# Same, with OneTerm's hot-path crates optimized (full-screen TUIs such as DOOM-fire
# are unusable at opt-level 0; see [profile.fast-dev] in the root Cargo.toml)
cargo run -p oneterm-app --profile fast-dev

# Test
cargo test --workspace
```

### 3.4. Theme & icon

- Theme: create a JSON file in `crates/theme/themes/`, then add it to the `BUILTIN_THEMES` list in `crates/theme/src/theme.rs`.
- Icon: OneTerm ships its own `AppIcon` enum (see `crates/theme/src/icon.rs`). Drop an SVG into `crates/theme/assets/icons/<name>.svg` — `crates/theme/build.rs` + the `icon_named!` macro auto-generate the `AppIcon::<PascalName>` variant (e.g. `arrow-right.svg` → `AppIcon::ArrowRight`). The gpui-component `IconName` (Lucide) set is also available for built-in icons (see `reference/gpui-component/crates/ui/src/icon.rs`).
- Do not hardcode colors in a component — read from `cx.theme()` / `TerminalTheme`.

## 3.5. Git & Commit

- Branch naming: `feat/<scope>`, `fix/<scope>`, `refactor/<scope>`, `docs/<scope>`.
- Commit message (Conventional Commits):

```
<type>(<scope>): <short description>

<body — explain "why", not just "what">

Refs: #issue
```

- alway respect `.gitignore`

---

## 4. Automated quality gate

Before completing a task, the agent **must** run and confirm the **same set of checks
CI runs** (`.github/workflows/ci.yml`). Run the bundled script (it stops at the first
failure and prints the failing command):

```bash
scripts/ci-local.sh          # bash / Git Bash
pwsh scripts/ci-local.ps1    # PowerShell (Windows)
```

The script runs, in order:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # also type-checks every target (no separate build step)
cargo test --workspace
python scripts/verify-dependency-graph.py     # crate graph policy + VERSION/Cargo.toml agreement
python scripts/check-ui-fork.py               # gpui-component vendor baseline
python scripts/check-doc-paths.py             # architecture doc paths
python -m unittest scripts/test_check_english.py
python scripts/check-english.py               # English-only contributor text
python scripts/completion-catalog.py validate # completion catalogs vs schema
python scripts/benchmark-scale.py --list      # scale benchmark manifest
python scripts/third-party-notices.py --check # THIRD-PARTY-NOTICES.md matches Cargo.lock
```

Optional (need network / extra tools; CI runs them too): `bash vendor/refresh.sh --check`
(vendored forks == pristine + patches) and `cargo deny check licenses bans advisories`
(`cargo install cargo-deny`). Pass `--full` to `ci-local` to include both.

If any command fails → fix it before reporting completion. Do not report a task done
with only fmt/clippy/build green.


## 5. Quick reference

| What you need to know | Where to read |
|---|---|
| Project structure (directory tree, conventions, dependency graph) | [`docs/agents/structure.md`](docs/agents/structure.md) |
| **Crate & dependency rules (R1–R12)** | [`docs/agents/crate-dependency-rules.md`](docs/agents/crate-dependency-rules.md) |
| Code conventions | [`docs/agents/code-style.md`](docs/agents/code-style.md) |
| Rev lock, dependencies, reference-first | [`docs/agents/dependencies.md`](docs/agents/dependencies.md) |
| **Terminal backend design** (local + ssh, alacritty render) | [`docs/terminal-backend.md`](docs/terminal-backend.md) |
| **Terminal Split design** (Spaces, split R/L/U/D, drag tab into Space) | [`docs/terminal-split.md`](docs/terminal-split.md) |
| SSH client connect / auth design | [`docs/ssh-client-connect.md`](docs/ssh-client-connect.md) |
| SFTP file browser design | [`docs/sftp-browser-design.md`](docs/sftp-browser-design.md) |
| SFTP-follows-terminal-CWD design | [`docs/sftp-follow-terminal-cwd.md`](docs/sftp-follow-terminal-cwd.md) |
| OSC sequence support checklist | [`docs/osc-sequences-checklist.md`](docs/osc-sequences-checklist.md) |
| Terminal rendering optimization | [`docs/terminal-rendering-optimization.md`](docs/terminal-rendering-optimization.md) |
| Terminal feature gap analysis | [`docs/terminal-gap-analysis.md`](docs/terminal-gap-analysis.md) |
| gpui-component API overview | `reference/gpui-component/CLAUDE.md` |
| Component list & source | `reference/gpui-component/crates/ui/src/` |
| Simple app example | `reference/gpui-component/examples/hello_world/` |
| DockArea example | `reference/gpui-component/examples/sidebar/src/main.rs` |
| Icon names | `reference/gpui-component/crates/ui/src/icon.rs` |
| Theme schema & colors | `reference/gpui-component/.theme-schema.json` |
| gpui internal skills | `reference/gpui-component/skills/` |
| Documentation (en) | `reference/gpui-component/docs/docs/` |
| Documentation index (current vs. archived docs) | [`docs/README.md`](docs/README.md) |

<!-- HARNESS:BEGIN -->
## Harness

Default: classify the request, establish its documentation record, make the smallest fitting change, verify it, then reconcile docs and report evidence and gaps.

Before editing implementation files, create or update one Markdown work packet with the outcome, owning docs, documentation action, acceptance, and verification plan. Every source, test, schema, script, or behavior-affecting configuration change requires a work packet generated from `docs/templates/work.md`.

A new capability requires a Spec Intake, accepted owning contract, and work packet before implementation. Every Spec Intake owns one mandatory High-Level Design (`high-level-design.md`, auto-generated with the intake); shape architecture, data ownership, and interfaces there. Detail (Low-Level) Design is optional and split by concern under the intake's `low-level-design/` folder, and required for the high-risk lane before any work packet. A bug or maintenance task still requires locating and reviewing the owning docs before implementation; update stale docs, or record the reviewed paths and a no-change reason in the packet. When you try the built work and ask to "fix points that did not pass" before it was accepted, that is acceptance rework of the **owning US/BUG** — reopen and rework that packet (`harness story reopen`), do not open a new BUG. A new BUG is only for defects in behavior already accepted/shipped. A detailed prompt may shorten these records but never removes them.

Read the affected source, owning docs, nearby decisions, and relevant tests. Read `docs/PROJECT.md` for project facts and standing invariants. Follow the editable files under `docs/templates/`. Create a decision only for choices future work must inherit and a trace when evidence or failure context must persist.

Treat auth, authorization, data loss, migrations, secrets, external effects, and public contracts as high-risk. Ask before implementing consequential ambiguous behavior. Never claim unverified behavior as working.

This pre-code documentation gate is mandatory and overrides less strict artifact-routing text in preserved project docs. Read `docs/HARNESS.md` for routing and completion details. Use the `harness` tool (or `/harness`) for durable records and mechanical checks; no external binary is required.
<!-- HARNESS:END -->
