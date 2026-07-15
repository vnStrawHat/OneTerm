# OneTerm

> GUI client for **SSH / SFTP / Local Shell**, written in **Rust** with [GPUI](https://github.com/zed-industries/zed) & [gpui-component](https://github.com/longbridge/gpui-component).

OneTerm is a terminal emulator plus host/session manager: connect to remote shells over SSH, browse and transfer files over SFTP, and open local shells. Terminal rendering is powered by `alacritty_terminal` and drawn with GPUI.

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

- Full ANSI / VT rendering via `alacritty_terminal`
- 16 / 256 / truecolor (24-bit) colors
- Hand-drawn box-drawing, block elements & powerline (crisp at any font size)
- Multiple cursor styles (block / bar / underline) + blink
- Mouse selection + copy/paste, right-click context menu
- **In-buffer search** (Ctrl+F) — match highlighting, next / previous (Enter / Shift+Enter), wrap-around
- Scrollback history (configurable, 10,000 lines by default) + custom terminal scrollbar
- URL detection (plain-text + OSC 8 hyperlinks) with hover highlight and click-to-open
- IME support (Vietnamese / CJK input)
- Bell (toggleable)
- Minimum-contrast — auto-boosts text/background contrast for readability
- Clipboard via OSC 52
- Desktop notifications (OSC 9) shown as toasts; taskbar progress (OSC 9;4)
- Shell integration: OSC 7 (cwd), OSC 133 (prompt markers / exit code), OSC 0/2 (title), OSC 4/104 palette overrides, OSC 10/11/12 dynamic colors

### 🪟 Terminal split (Spaces)

- Split a single terminal tab into resizable **Spaces** — Right / Left / Up / Down
- Recursive nesting (binary pane tree), like tmux / Zed panes
- Resizable split handles; the active Space is highlighted
- Fill an empty Space by dragging a terminal tab onto it (move semantics)
- "New Terminal Here" spawns a local shell in an empty Space
- Context-menu driven; closing down to one Space reverts the tab to a plain single terminal

### 🔌 SSH connectivity

- SSH client based on `russh` (hidden tokio runtime)
- **Password**, **private key** (with passphrase), **SSH agent**, and **no-auth** authentication
- Interactive shell channel with auto-resize to the window
- Optional shell-integration injection (OSC 7 cwd + OSC 133 markers) for servers whose shell doesn't emit them
- Bandwidth accounting (network speed indicator)
- Passwords kept in RAM only, **never** written to disk (masked in logs)

### 📁 SFTP file browser

- Browse remote directories with breadcrumb navigation
- Columns: Name / Date Modified / Permissions / Size / Owner / Group
- Sort by column (folders always listed before files)
- Resize & show/hide columns (persisted across sessions)
- Upload files & folders, download files
- Rename / Delete / Create new folder
- View properties — permissions shown as `drwxr-xr-x (0775)`
- Transfer queue with progress bars & cancellation
- **Sync to terminal CWD** — one click jumps the browser to the active SSH session's current directory (via OSC 7)
- SFTP runs over the same open SSH connection

### 🗂️ Session management

- Tree-based session list with groups
- Connect / add-new-session dialog
- Rename groups, assign colors to sessions
- Search / filter sessions
- Persisted to `ssh_session.json` (passwords never stored)

### 🧩 Layout & UI

- Flexible DockArea (left / right / bottom docks + center tabs)
- Multiple concurrent sessions across tabs
- Title bar + menu bar (File / Edit / View / ...)
- Status bar with a date/time clock, network speed indicator, and CPU/memory resource indicator
- Zoom / fullscreen the active panel (Shift+Esc)
- Quick close panel (Ctrl+W)
- Remembers dock layout across sessions (`docks.json`)

### ⚙️ Settings & theming

- **Settings window** (Ctrl+,) with pages: General / Terminal / Appearance / About
- General: UI font size + configurable, press-to-rebind key bindings (persisted to `ui_config.json`)
- Terminal: font, cursor, layout/padding, shell, scroll, bell, colors, security groups
- Appearance: theme mode + theme picker
- **24 built-in themes** (2 Zed + 22 from the gpui-component collection: adventure, alduin, asciinema, aurora, ayu, catppuccin, everforest, fahrenheit, flexoki, gruvbox, harper, hybrid, jellybeans, kibble, macos-classic, matrix, mellifluous, molokai, solarized, spaceduck, tokyonight, twilight)
- Terminal configuration via `terminal.json` (supports `//` and `/* */` comments)
- Colors read from the theme, never hardcoded in components

### 💻 Local shell

- Local PTY via `alacritty_terminal::tty` + custom `EventLoop`
- **Windows:** ConPTY (bundled `OpenConsole.exe` + `conpty.dll`) — fully tested
- Unix local PTY code path compiles but is **not yet tested** on Linux/macOS
- Shell auto-detection (`cmd` / `pwsh` / `COMSPEC` on Windows)

### 📦 Packaging & platforms

- **Windows** is the primary, fully-supported platform
- Linux / macOS compile but are **not yet tested** (see Platform support above)
- Optimized release build (fat LTO, single codegen unit, stripped symbols)
- Embedded app icon + version info in `.exe` (Windows)

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
- [`docs/osc-sequences-checklist.md`](docs/osc-sequences-checklist.md) — OSC sequence support checklist
