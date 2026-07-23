# OneTerm

> GUI client for **SSH / SFTP / Local Shell**, written in **Rust** with [GPUI](https://github.com/zed-industries/zed) & [gpui-component](https://github.com/longbridge/gpui-component).

OneTerm is a terminal emulator plus host/session manager with a **Zed-style workspace UI**: connect to remote shells over SSH, browse and transfer files over SFTP, open local shells, and monitor coding agents in a live Agent Panel fed by the OSC 9;7 proposal ([spec](docs/osc-agent-status.md)). Terminal rendering is powered by `alacritty_terminal` and drawn with GPUI.

**Version:** 0.1.0 · **Edition:** Rust 2024

> ### 🪟 Platform support
>
> OneTerm is **developed and tested primarily on Windows** — that is the
> first-class, fully-supported platform (local shells use Windows ConPTY,
> the release build embeds an app icon + version info, and runtime assets
> such as `conpty.dll` / `OpenConsole.exe` are bundled).
>
> Linux and macOS **compile** and the cross-platform code paths are in
> place, but they are **not yet tested**. Expect rough edges on those
> platforms (local PTY, packaging, theming) until they receive a proper
> QA pass. PRs improving Linux/macOS support are welcome.

---

## ✨ Features

### 🖥️ Terminal emulator

- ANSI / VT rendering via `alacritty_terminal`
- 24-bit colors, box drawing, and multiple cursor styles
- Mouse selection, search, scrollback, and custom scrollbar
- URL / OSC 8 detection, IME support, clipboard via OSC 52, bell
- OSC 9 toasts, OSC 9;4 progress, and shell integration (OSC 7 / 133 / 0 / 2 / 4 / 104 / 10 / 11 / 12)

### 🪟 Terminal split (Spaces)

- Resizable split Spaces (Right / Left / Up / Down) with nesting
- Drag a terminal tab into an empty Space to fill it
- Context-menu driven; closing to one Space restores a single terminal

### 🔌 SSH connectivity

- SSH client based on `russh`
- Password, private key, SSH agent, and no-auth authentication
- Auto-resize shell channel with optional shell integration
- Bandwidth indicator; passwords stay in RAM only

### 📁 SFTP file browser

- Browse remote directories with breadcrumbs and sortable columns
- Upload / download, rename / delete, create folders, view properties
- Transfer queue with progress bars and cancellation
- Sync the browser to the active SSH session CWD via OSC 7

### 🗂️ Session management

- Grouped session tree with connect, rename, color, and search
- Persisted to `ssh_session.json` (passwords never stored)

### 🤖 Agent Panel

- Right-dock fleet view of coding agents
- Currently supports Pi Coding Agent (`pi install npm:@vnstrawhat/pi-oneterm`)
- Built from the OSC 9;7 proposal and folded into a global Agent Registry
- Shows working, blocked, idle, done, and error

### 🧩 Layout & UI

- Zed-style workspace with a flexible DockArea, left / right / bottom docks, and center tabs
- Multiple concurrent sessions across tabs
- Title bar, menu bar, status bar, zoom, and quick close
- Remembers dock layout across sessions (`docks.json`)

### ⚙️ Settings & theming

- Settings window (General / Terminal / Appearance / About)
- UI font size, key bindings, terminal tuning, and themes
- Terminal configuration in `terminal.json`
- Colors read from the theme, never hardcoded in components

### 💻 Local shell

- Local PTY via `alacritty_terminal::tty`
- Windows ConPTY is bundled; Unix local PTY compiles but is untested
- Windows is the primary platform; Linux/macOS compile but are untested

### 📦 Packaging & platforms

- Optimized release build with embedded app icon + version info on Windows

## 🚀 Build & run

Requires: Rust toolchain (edition 2024).

```bash
# Run the app (dev binary = oneterm-debug, keeps the console for logs)
cargo run -p oneterm-app

# Build the whole workspace
cargo build --workspace

# Format + lint (must be warning-free)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace
```

### Debug logging and terminal diagnostics

OneTerm uses `env_logger` and reads standard `RUST_LOG` directives. The normal development
run uses the `oneterm-debug` binary and enables application DEBUG logs without enabling the
extra terminal timing instrumentation.

PowerShell:

```powershell
# General application debug logs
$env:RUST_LOG = "info,gpui=info,oneterm=debug"
cargo run -p oneterm-app

# Add terminal renderer and PTY timing diagnostics
$env:RUST_LOG = "info,gpui=info,oneterm=debug"
cargo run -p oneterm-app --features terminal-diagnostics

# Remove the override when finished
Remove-Item Env:RUST_LOG
```

Command Prompt (`cmd.exe`):

```cmd
set "RUST_LOG=info,gpui=info,oneterm=debug"
cargo run -p oneterm-app --features terminal-diagnostics
```

Bash (Linux, macOS, or Git Bash):

```bash
RUST_LOG="info,gpui=info,oneterm=debug" \
  cargo run -p oneterm-app --features terminal-diagnostics
```

The `terminal-diagnostics` feature is intentionally opt-in. It enables `[TerminalElement]`
renderer latency reports and `[PTY pump]` lock timing reports. The renderer reports after
its first painted frame and then at most once every five seconds while frames are painted;
the PTY pump reports over two-second sampling windows. These records use DEBUG logging, so
`oneterm=debug` is sufficient. See
[`docs/review/performance-benchmark.md`](docs/review/performance-benchmark.md) for the
workload and measurement protocol.

### Release build

```powershell
# Windows (embeds icon + version info, stages into dist/)
pwsh scripts/build-release.ps1
pwsh scripts/build-release.ps1 -Target aarch64-pc-windows-msvc
```

```bash
# Linux / macOS — untested; see Platform support note
./scripts/build-release.sh
TARGET=aarch64-unknown-linux-gnu ./scripts/build-release.sh
```

Packaged output lands in `dist/oneterm-<triple>/`:
- **Windows** — `oneterm.exe` plus the runtime assets (`conpty.dll` + `x64/OpenConsole.exe`);
  the exe has the app icon + version info embedded (build.rs).
- **macOS** — `OneTerm.app` bundle (double-click to launch **without** an extra
  Terminal.app window). On macOS a raw GUI binary is treated as a CLI tool, so
  Finder opens Terminal.app to run it; packaging it inside a `.app` bundle with
  an `Info.plist` (`NSPrincipalClass=NSApplication`) makes LaunchServices launch
  it directly — the macOS analog of the Windows `windows_subsystem = "windows"`
  fix. The `.icns` icon is generated best-effort from the Windows `.ico`.
- **Linux** — `oneterm` plus optional `terminal.json` / `docks.json` defaults.

---

## 📚 Documentation

- [`AGENTS.md`](AGENTS.md) — developer & AI-agent guide
- [`docs/agents/structure.md`](docs/agents/structure.md) — project structure & dependency graph
- [`docs/agents/crate-dependency-rules.md`](docs/agents/crate-dependency-rules.md) — hard crate & dependency rules (R1–R12)
- [`docs/agents/code-style.md`](docs/agents/code-style.md) — code conventions
- [`docs/agents/dependencies.md`](docs/agents/dependencies.md) — dependencies & rev lock
- [`docs/terminal-backend.md`](docs/terminal-backend.md) — terminal backend design (local + ssh)
- [`docs/terminal-split.md`](docs/terminal-split.md) — terminal split (Spaces) design
- [`docs/ssh-client-connect.md`](docs/ssh-client-connect.md) — SSH connection / auth design
- [`docs/sftp-browser-design.md`](docs/sftp-browser-design.md) — SFTP file browser design
- [`docs/sftp-follow-terminal-cwd.md`](docs/sftp-follow-terminal-cwd.md) — SFTP-follows-terminal-CWD design
- [`docs/osc-agent-status.md`](docs/osc-agent-status.md) — OSC 9;7 agent status proposal/spec
- [`docs/osc-sequences-checklist.md`](docs/osc-sequences-checklist.md) — OSC sequence support checklist
