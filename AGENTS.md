# AGENTS.md — OneTerm

> Guide for AI agents (and contributors) when working with the **OneTerm** project — an SSH / SFTP / LocalShell GUI client written in Rust + [gpui-component](https://github.com/longbridge/gpui-component).

## Table of contents

| # | Topic | File |
|---|---|---|
| 1 | Project introduction & core principles | `AGENTS.md` (this file) |
| 2 | Project structure (directory tree, conventions, dependency graph) | [`docs/agents/structure.md`](docs/agents/structure.md) |
| 3 | Development guide (workflow, commands, git) | `AGENTS.md` |
| 4 | Code conventions (style, GPUI, async, error) | [`docs/agents/code-style.md`](docs/agents/code-style.md) |
| 5 | Dependencies, rev lock, gpui-component integration, reference-first research | [`docs/agents/dependencies.md`](docs/agents/dependencies.md) |
| 6 | Roadmap & quick ref | `AGENTS.md` |

> 📚 **Required reading** before writing code:
> 1. `AGENTS.md` (this file) — to understand the overview, workflow, git, and quality gate.
> 2. [`docs/agents/structure.md`](docs/agents/structure.md) — to learn the crate structure, directory tree, and dependency graph.
> 3. [`docs/agents/code-style.md`](docs/agents/code-style.md) — to learn the code conventions (GPUI, async, error).
> 4. [`docs/agents/dependencies.md`](docs/agents/dependencies.md) — to learn the rev lock & reference-first research.

---

## 1. Project introduction

**OneTerm** is a cross-platform GUI client (macOS / Linux / Windows) that provides:

- **SSH client** — connect to remote shells (russh).
- **SFTP client** — browse, upload, and download remote files.
- **LocalShell** — run a local shell over a PTY.
- **Terminal emulator** — render ANSI/VT, colors, scrollback.
- **Host manager** — store and manage the host list and credentials.
- **Session tabs** — open multiple sessions simultaneously in a workspace dock.
- **Settings / themes** — configure font, colors, key bindings, theme.

### Core principles

1. **Clear layer separation** — the UI contains no protocol logic; protocols know nothing about the UI.
2. **One crate per domain** — each crate has a single responsibility.
3. **Async-first** — all I/O (network, PTY, file) runs asynchronously via `cx.spawn` / `smol` / `tokio`.
4. **Type-safe state** — use GPUI's `Entity<T>`; avoid sharing `Rc<RefCell<…>>` at the application layer.
5. **Reference-first research** — when you need to understand APIs, code examples, or docs for `gpui` / `gpui-component`, **always read from `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\`**. See [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md) for details.
6. **English-only** — all code, comments, doc comments, commit messages, docs, and any written content in the repository **must be in English**. Do not write Vietnamese (or any other non-English language) anywhere in the codebase. This rule has **zero exceptions**; if you catch yourself writing a non-English comment, rewrite it in English before continuing.
7. **Scoped searches only** — every filesystem/search-tool invocation **must be scoped to a specific directory or path** (`crates/`, `reference/`, `docs/`, a single file, …). Never run a rootless or disk-wide search: `find /`, `grep -r /`, `rg` from the filesystem root, or any tool that scans the entire disk. These are extremely slow and can be destructive. Pick a concrete project-relative path before searching.

> 📂 **Project structure** (directory tree, file organization conventions, and the inter-crate dependency graph) is split into a separate file: see [`docs/agents/structure.md`](docs/agents/structure.md).

---

## 3. Development guide

### 3.0. Learning the gpui-component API — read the reference first

Before writing UI code, use `find` / `grep` / `read` on `D:\TrungKFC-Research\Rust\myTerm2\reference\gpui-component\` to look up APIs. See [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md) for the detailed lookup table. **Do not** use `web_search` for gpui-component unless the reference is missing something.

### 3.1. Common commands

> ⚠️ **Safety boundary**: Every command below **must only be run inside** `D:\TrungKFC-Research\Rust\myTerm2`. Do not `cd` outside it, and do not run `cargo init` / `git clone` in any other directory.

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

# Test
cargo test --workspace
```

**Do not run**:

- ❌ `cargo init` / `cargo new` (the project already exists).
- ❌ `git clone` outside the workspace.
- ❌ Any command that modifies files outside `D:\TrungKFC-Research\Rust\myTerm2`.
- ❌ `rm -rf` without a guard path.
- ❌ **Rootless / disk-wide searches** — `find /`, `grep -r /`, `rg` from the FS root, or any tool called without an explicit scope. **ALWAYS pass a concrete directory path** (e.g. `find crates/ui/src -name '*.rs'`, `grep -rn 'foo' crates/`). This is a hard rule — see Core principle 7.

### 3.2. Feature workflow

1. **Read first** `reference/gpui-component/CLAUDE.md` and the corresponding code in `reference/gpui-component/` before writing UI.
2. **Read** [`docs/agents/structure.md`](docs/agents/structure.md) to know which crate contains what + the dependency rules.
3. **Read** [`docs/agents/code-style.md`](docs/agents/code-style.md) to grasp the code conventions.
4. **Update `core` first** if you need to add a domain type / trait.
5. **Implement** in the appropriate crate (`ssh` / `local` / `ui`).
6. **Wire it** into `ui::state::AppState` and `layout::workspace`.
7. Run `cargo fmt && cargo clippy --workspace --all-targets -- -D warnings`.
8. Commit with a Conventional Commits message (see section 4).

### 3.3. When adding a new gpui-component

- Place the file in `crates/ui/src/components/<name>.rs`.
- If it is a stateless widget → prefer `RenderOnce`.
- If it needs state → `Render` + `Entity<T>` for the state, exposed via `cx.new(|_| State::new())`.
- Write a `///` doc comment for every public item.

### 3.4. Theme & icon

- Theme: create a JSON file in `crates/ui/themes/`, then add it to the `BUILTIN_THEMES` list in `crates/ui/src/theme.rs`.
- Icon: OneTerm ships its own `AppIcon` enum (see `crates/ui/src/icon.rs`). Drop an SVG into `crates/ui/assets/icons/<name>.svg` — `build.rs` + the `icon_named!` macro auto-generate the `AppIcon::<PascalName>` variant (e.g. `arrow-right.svg` → `AppIcon::ArrowRight`). The gpui-component `IconName` (Lucide) set is also available for built-in icons (see `reference/gpui-component/crates/ui/src/icon.rs`).
- Do not hardcode colors in a component — read from `cx.theme()` / `TerminalTheme`.

---

## 4. Git & Commit

- Branch naming: `feat/<scope>`, `fix/<scope>`, `refactor/<scope>`, `docs/<scope>`.
- Commit message (Conventional Commits):

```
<type>(<scope>): <short description>

<body — explain "why", not just "what">

Refs: #issue
```

- **Do not commit**: `target/`, `Cargo.lock` if it is a library (workspace binaries still commit `Cargo.lock`), `reference/`, `.pi/`.
- `.gitignore` already excludes `target/`, `reference/`, `.pi/`. Keep it as is.

---

## 5. Automated quality gate

Before completing a task, the agent **must** run and confirm the following pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
```

If any of the three commands above fails → fix it before reporting completion.

### 5.1. Release build

> OneTerm is a **binary**, not a library → commit `Cargo.lock`.

The release profile is pre-configured in the workspace `Cargo.toml`:
`opt-level=3`, `lto="fat"`, `codegen-units=1`, `strip="symbols"`, `overflow-checks=false`.

**Windows** (native, embeds the app icon + version info into the exe):

```powershell
pwsh scripts/build-release.ps1                      # build host triple, stage dist/
pwsh scripts/build-release.ps1 -Target aarch64-pc-windows-msvc
pwsh scripts/build-release.ps1 -NoDist               # build only, do not stage dist/
```

**Linux / macOS**:

```bash
./scripts/build-release.sh
TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
```

When done, the clean packaged build lives in `dist/oneterm-<triple>/` containing:
`oneterm(.exe)` + `conpty.dll` + `x64/OpenConsole.exe` (Windows) + `terminal.json`/`docks.json`.
(The dev build produces `oneterm(.exe)`; build-release renames it to `oneterm(.exe)` when staging dist.)

> The app icon (`assets/icons/terminal-48x48.ico`, `assets/icons/terminal-96x96.ico`)
> is embedded into the exe via `crates/app/assets/oneterm.rc` + `embed-resource`,
> compiled in `crates/app/build.rs`. No manual post-build step is needed.
> 📌 **Reminder**: All commands must be run inside `D:\TrungKFC-Research\Rust\myTerm2`. If the agent needs to inspect files in `reference/`, use a **relative path** from the workspace root; do not `cd` outside it.
>
> 📚 **gpui-component lookup**: before using `web_search` / `fetch_content` / `code_search` for information about gpui / gpui-component, **you must read inside `reference/gpui-component/` first** — see [`docs/agents/dependencies.md` § 5](docs/agents/dependencies.md).

---

## 6. Expansion roadmap (status)

- [x] Workspace skeleton + Cargo.toml + .gitignore + .rustfmt.toml + workspace lints.
- [x] `core`: types + traits (`TerminalSession`, `SftpBackend`, `AppError`).
- [x] `local`: a PTY shell that works inside a gpui view.
- [x] `ssh`: connect with password + pubkey + agent, shell channel.
- [x] `ssh/sftp`: list / upload / download (via `SftpSession`).
- [x] `ui`: layout (dock + statusbar + title bar + app menus) + session tabs + SFTP panel + terminal view/settings.
- [ ] `ui`: host manager + host-list sidebar (host list currently lives in `session_tabs`).
- [x] General Settings UI (font, theme, key bindings) — wraps the gpui-component `Settings` widget (General / Terminal / Appearance / About pages); UI font size + theme persist to `ui_config.json` (via a `Theme` observer), key bindings are press-to-rebind (persist to `ui_config.json`); opens in a separate window (Ctrl-,). The exe icon uses numeric resource ID 1 so gpui's Windows window-icon loader resolves it (all windows get the app icon).
- [ ] i18n (en, vi) — `rust-i18n` is wired in the workspace, but no locale files yet.
- [ ] CI: build & test on macOS / Linux / Windows (a `release.yml` workflow exists; full cross-platform CI matrix pending).
- [ ] Package an installer (cargo-bundle or cargo-dist).

---

## 7. Quick reference

### 7.1. Quick-jump paths

| What you need to know | Where to read |
|---|---|
| Project structure (directory tree, conventions, dependency graph) | [`docs/agents/structure.md`](docs/agents/structure.md) |
| Code conventions (style, GPUI, async, error) | [`docs/agents/code-style.md`](docs/agents/code-style.md) |
| Rev lock, dependencies, reference-first | [`docs/agents/dependencies.md`](docs/agents/dependencies.md) |
| **Terminal backend design** (local + ssh, alacritty render) | [`docs/terminal-backend.md`](docs/terminal-backend.md) |
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

### 7.2. FAQ

**Q: Where should a new Rust file go?**
A: See [`docs/agents/structure.md`](docs/agents/structure.md). There are 5 crates (`app`, `core`, `ssh`, `local`, `ui`) with strict dependency rules; the UI must not import from `ssh`/`local`.

**Q: I need to add a new gpui component — where do I start?**
A: Read [`docs/agents/code-style.md` § 2](docs/agents/code-style.md), then `grep -rn "<component name>" reference/gpui-component/crates/ui/src/` to find the API.

**Q: I want to add a new Rust crate?**
A: See [`docs/agents/dependencies.md` § 3](docs/agents/dependencies.md). If the crate already exists in `reference/gpui-component/Cargo.toml` → use the locked rev. If not → open an issue first.

**Q: gpui-component just released a new version — how do I update?**
A: See [`docs/agents/dependencies.md` § 4 and 5.5](docs/agents/dependencies.md).