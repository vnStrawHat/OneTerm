# OneTerm

> Cross-platform GUI client for **SSH / SFTP / Local Shell**, written in **Rust** with [GPUI](https://github.com/zed-industries/zed) & [gpui-component](https://github.com/longbridge/gpui-component).

OneTerm is a terminal emulator plus host/session manager: connect to remote shells over SSH, browse and transfer files over SFTP, and open local shells (ConPTY on Windows). Terminal rendering is powered by `alacritty_terminal` and drawn with GPUI.

---

## ✨ Features

### 🖥️ Terminal emulator

- [x] Full ANSI / VT rendering via `alacritty_terminal`
- [x] 16 / 256 / truecolor (24-bit) colors
- [x] Hand-drawn box-drawing, block elements & powerline (crisp at any font size)
- [x] Multiple cursor styles (block / bar / underline) + blink
- [x] Mouse selection + copy/paste
- [x] **Vi mode** — keyboard-driven motion & selection (Ctrl+Shift+Space)
- [x] Scrollback history (10,000 lines by default, configurable)
- [x] Custom terminal-specific scrollbar
- [x] URL detection with click-to-open
- [x] IME support (Vietnamese / CJK input)
- [x] Bell (toggleable)
- [x] Minimum-contrast — auto-boosts text/background contrast for readability
- [x] Clipboard via OSC 52
- [x] Shell integration (OSC 7 cwd, OSC 133 prompt / exit code)

### 🔌 SSH connectivity

- [x] SSH client based on `russh`
- [x] **Password** authentication
- [x] **Private key** authentication (with passphrase)
- [x] **SSH agent** authentication
- [x] No-auth (None) connections
- [x] Interactive shell channel with auto-resize to the window
- [x] Passwords kept in RAM only, **never** written to disk (masked in logs)

### 📁 SFTP file browser

- [x] Browse remote directories with breadcrumb navigation
- [x] Columns: Name / Date Modified / Permissions / Size / Owner / Group
- [x] Sort by column (folders always listed before files)
- [x] Resize & show/hide columns (persisted across sessions)
- [x] Upload files & folders
- [x] Download files
- [x] Rename / Delete / Create new folder
- [x] View properties — permissions shown as `drwxr-xr-x (0775)`
- [x] Transfer queue with progress bars & cancellation
- [x] SFTP runs over the same open SSH connection

### 🗂️ Session management

- [x] Tree-based session list with groups
- [x] Connect / add-new-session dialog
- [x] Rename groups, assign colors to sessions
- [x] Search / filter sessions
- [x] Persisted to `ssh_session.json` (passwords never stored)

### 🧩 Layout & UI

- [x] Flexible DockArea (left / right / bottom docks + center tabs)
- [x] Multiple concurrent sessions across tabs
- [x] Title bar + menu bar (File / Edit / View / ...)
- [x] Status bar with a date/time clock + network speed indicator
- [x] Zoom / fullscreen a panel (Shift+Esc)
- [x] Quick close panel (Ctrl+W)
- [x] Remembers dock layout across sessions (`docks.json`)

### ⚙️ Configuration & theming

- [x] Terminal configuration via `terminal.json` (supports `//` and `/* */` comments)
- [x] Config groups: font, cursor, layout, shell, scroll, bell, colors
- [x] In-app terminal settings panel
- [x] 2 built-in themes: **Zed One Dark** & **Zed One Light**
- [x] Colors read from the theme, never hardcoded in components

### 💻 Local shell

- [x] Local PTY via `alacritty_terminal::tty`
- [x] ConPTY on Windows (bundled `OpenConsole.exe` + `conpty.dll`)
- [x] Shell auto-detection (`cmd` / `pwsh` / `COMSPEC`)

### 📦 Packaging & platforms

- [x] Cross-platform: Windows / Linux / macOS
- [x] Optimized release build (fat LTO, stripped symbols)
- [x] Embedded app icon + version info in `.exe` (Windows)
- [ ] Full settings UI (general / appearance / about)
- [ ] i18n (en / vi)
- [ ] CI build & test across platforms
- [ ] Installer (cargo-bundle / cargo-dist)

---

## 🏗️ Architecture

The workspace consists of 5 crates with strict layering (the UI holds no protocol logic):

| Crate | Responsibility |
|---|---|
| `app` | Binary entry point — init GPUI, open the window, mount the workspace |
| `core` | Domain types & traits (`TerminalSession`, `SftpBackend`), terminal helpers — no GPUI dependency |
| `local` | Local shell over PTY / ConPTY |
| `ssh` | SSH + SFTP client based on `russh` (hidden tokio runtime) |
| `ui` | All GPUI: layout, views, theme, state |

> Dependency rule: `app → {ui, ssh, local, core}`, and `ui / ssh / local → core`. The `ui` crate does **not** import `ssh` / `local` directly — it communicates only through traits.

See [`docs/agents/structure.md`](docs/agents/structure.md) and [`docs/terminal-backend.md`](docs/terminal-backend.md) for details.

---

## 🚀 Build & run

Requires: Rust toolchain (edition 2024).

```bash
# Run the app (dev)
cargo run -p app

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
```

```bash
# Linux / macOS
./scripts/build-release.sh
```

Packaged output lands in `dist/oneterm-<triple>/`.

---

## 📚 Documentation

- [`AGENTS.md`](AGENTS.md) — developer & AI-agent guide
- [`docs/agents/structure.md`](docs/agents/structure.md) — project structure
- [`docs/agents/code-style.md`](docs/agents/code-style.md) — code conventions
- [`docs/agents/dependencies.md`](docs/agents/dependencies.md) — dependencies & rev lock
- [`docs/terminal-backend.md`](docs/terminal-backend.md) — terminal backend design
