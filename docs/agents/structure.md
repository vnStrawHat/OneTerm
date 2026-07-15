# Project structure — OneTerm

> File split from `AGENTS.md` (section 2). Describes the workspace structure, the canonical directory tree, and the Rust file organization conventions.
>
> 📌 The tree in §1 reflects the **actual on-disk state** (current commit). **Planned** parts (not yet created) are listed separately in §5 as a roadmap.

## 1. Directory tree (actual state)

OneTerm is a **layered** workspace. The UI is split into low shared layers, a
feature-agnostic app shell, and four feature crates; the two protocol backends
(`ssh` / `local-shell`) are known **only** to the `app` binary. See §3 for the crate
responsibilities and the dependency graph.

```
OneTerm/
├── Cargo.toml                      # Workspace root — members + workspace deps + lints
├── Cargo.lock
├── AGENTS.md                       # Entry-point file for the agent
├── README.md                       # Project README
├── VERSION                         # Single-source version string (read by build scripts)
├── .gitignore
├── .rustfmt.toml
│
├── crates/
│   ├── app/                        # Binary + wiring: the ONLY crate that knows every other crate
│   │   ├── Cargo.toml              # name = "oneterm-app", default-run = "oneterm-debug"
│   │   ├── build.rs                # Embed app icon (.rc) + copy conpty.dll/OpenConsole.exe
│   │   ├── assets/                 # Runtime resources (oneterm.rc, conpty.dll, x64/OpenConsole.exe, icons/)
│   │   ├── macos/Info.plist        # macOS .app bundle descriptor ({{VERSION}})
│   │   └── src/
│   │       ├── lib.rs              # run(): logging + gpui init + install factory + init() + open window
│   │       ├── init.rs             # Aggregator: globals + feature init() + WorkspaceCommands assembly
│   │       ├── session_factory.rs  # AppSessionFactory: dispatches spawn_local/connect_ssh to local/ssh
│   │       ├── assets.rs           # CustomAssets (merges oneterm_theme::icon::UiAssets + gpui-component)
│   │       ├── window.rs           # open_window(cx) — create window + attach OneTermWorkspace
│   │       └── bin/
│   │           ├── oneterm.rs          # Release binary → oneterm(.exe) (WINDOWS subsystem)
│   │           └── oneterm-debug.rs    # Dev binary → oneterm-debug(.exe) (keeps console)
│   │
│   ├── core/                       # Domain model — leaf crate (no gpui, no alacritty)
│   │   ├── Cargo.toml              # name = "oneterm-core"
│   │   └── src/
│   │       ├── lib.rs              # Re-export AppError, LocalShellConfig, ShellKind, SftpBackend, SshConfig…
│   │       ├── error.rs            # AppError (thiserror) + Result<T>
│   │       ├── sftp.rs             # SftpBackend trait + FileEntry + FileStat
│   │       ├── ssh_config.rs       # SshConfig + SshAuthMethod (shared connect params; masked Debug)
│   │       └── config/             # Local shell config
│   │           ├── mod.rs
│   │           └── shell.rs        # LocalShellConfig + ShellKind + resolve_shell
│   │
│   ├── terminal/                   # Terminal ENGINE (alacritty-coupled, no gpui) — `oneterm-terminal`
│   │   ├── Cargo.toml              # deps: core, alacritty_terminal, async-channel, linkify, base64
│   │   └── src/
│   │       ├── lib.rs              # Re-export TerminalSession, SessionEvent, TerminalContent, PtySize…
│   │       ├── factory.rs          # PtySize + SessionFactory trait + install/get process global
│   │       ├── session.rs          # TerminalSession trait + SessionEvent + CursorBounds + NetStats
│   │       ├── content.rs / palette.rs / colors_util.rs / key_encode.rs / mouse_encode.rs
│   │       ├── osc.rs / osc_color.rs / url.rs / search.rs / paste.rs / security_policy.rs …
│   │       └── test_support.rs     # FakeTerminalSession (feature "test-support")
│   │
│   ├── highlight/                  # `oneterm-highlight` — semantic syntax highlighting engine
│   │
│   ├── ssh/                        # `oneterm-ssh` — russh client + SFTP (hidden tokio runtime)
│   │   └── src/                    # SshSession (impl TerminalSession), SftpSession (impl SftpBackend),
│   │                               #   listener/handler/task/state; config.rs re-exports core's SshConfig
│   │
│   ├── local-shell/                # `oneterm-local-shell` — local PTY (alacritty_terminal::tty + ConPTY)
│   │   └── src/                    # LocalSession (impl TerminalSession) + LocalListener + event_loop
│   │
│   ├── actions/                    # `oneterm-actions` — leaf: gpui action structs (Copy/Paste/AddPanel…)
│   │   └── src/lib.rs
│   │
│   ├── settings/                   # `oneterm-settings` — config load/save + live settings (no gpui-component)
│   │   └── src/
│   │       ├── lib.rs              # Re-export TerminalConfig, TerminalSettings, UiConfig + types
│   │       ├── terminal_config/    # terminal.json load/save (font/cursor/layout/scroll/bell/colors/security)
│   │       ├── terminal_settings/  # Live TerminalSettings + apply/persist/mutators/color helpers
│   │       └── ui_config.rs        # UiConfig (ui_font_size, theme_name, key_bindings) → ui_config.json
│   │
│   ├── state/                      # `oneterm-state` — shared global Entity<T> state + cross-cutting helpers
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app_state.rs        # AppState (global)
│   │       ├── notif_ext.rs        # Notification helpers
│   │       ├── commands.rs         # WorkspaceCommands fn-pointer registry (shell → feature inversion)
│   │       ├── dock_util.rs        # DockArea walking + set_right_dock_open (shared shell/feature helper)
│   │       ├── active_terminal.rs  # ActiveTerminalMetricsProvider (breadcrumb/net stats injection)
│   │       └── paths.rs            # docks.json path + SFTP_TABLE_STATE_FIELD (shared shell/sftp)
│   │
│   ├── theme/                      # `oneterm-theme` — theme registry + AppIcon (has build.rs)
│   │   ├── build.rs                # Sets ONETERM_UI_ICONS_DIR for the icon_named! macro
│   │   ├── assets/icons/           # OneTerm SVG icons (auto-generate AppIcon variants)
│   │   ├── themes/                 # 24 built-in JSON themes (2 Zed + 22 gpui-component)
│   │   └── src/{lib.rs, theme.rs, icon.rs}
│   │
│   ├── workspace/                  # `oneterm-workspace` — feature-AGNOSTIC app shell (has build.rs)
│   │   ├── build.rs                # Publishes ONETERM_VERSION (About action)
│   │   └── src/
│   │       ├── lib.rs              # Re-export OneTermWorkspace + save_dock_state_on_close
│   │       ├── layout/             # title_bar, app_menus, statusbar, workspace/{mod,actions,layout,persistence,zoom}
│   │       │                       #   builds feature panels by NAME via gpui-component PanelRegistry
│   │       └── widgets/            # statusbar widgets (breadcrumb, net_speed, datetime_clock, resource)
│   │
│   ├── terminal-view/              # `oneterm-terminal-view` — TERMINAL feature (has terminal-diagnostics feat)
│   │   ├── assets/highlight/       # default.json semantic style asset (include_str!)
│   │   └── src/                    # lib.rs init() (register terminal + terminal-settings panels + status
│   │                               #   metrics); panel/, view/, render/, element/, layout/, cell/,
│   │                               #   box_drawing/, handlers/, theme/, url/, highlight/, space/, search…
│   │
│   ├── sftp-ui/                    # `oneterm-sftp-ui` — SFTP feature (file browser + transfer queue)
│   │   └── src/                    # lib.rs init() (register "sftp" panel); panel/render/transfer/table…
│   │
│   ├── session-ui/                 # `oneterm-session-ui` — session tree + connect dialogs
│   │   └── src/                    # lib.rs init() (SshSessionStore::init + register "session" panel);
│   │                               #   panel, connect_dialog, quick_connect_dialog, session_state.rs …
│   │
│   └── settings-ui/                # `oneterm-settings-ui` — General Settings window (has build.rs)
│       ├── build.rs                # Publishes ONETERM_VERSION (About page)
│       └── src/                    # lib.rs: open_settings + setup_key_bindings commands;
│                                   #   panel/window/general/terminal/appearance/about/key_bindings …
│
├── docs/                           # Development documentation
│   ├── refactor/ui-crate-restructure.md   # This restructure's authoritative plan
│   ├── terminal-backend.md / ssh-client-connect.md / sftp-browser-design.md …
│   └── agents/{code-style.md, dependencies.md, structure.md (this file)}
│
└── reference/                      # Local clone of gpui-component (gitignored)
    └── gpui-component/
```

## 2. Structure conventions

- **Each Rust file is at most ~400 lines.** If it exceeds that → split into a submodule immediately. Conversely, **do not split too small**. Each file must have enough "responsibility mass" to stand on its own.
- **One module, one responsibility.** The file name = the main module name (snake_case).
- **One crate per layer / feature.** Shared logic goes in a low crate (`core` / `terminal` / `actions` / `settings` / `state` / `theme`); each user-facing feature is its own `*-ui` crate; the shell (`workspace`) is feature-agnostic.
- **Feature crates never depend on each other's internals** except the acyclic edge `session-ui → terminal-view` (a new SSH session opens a `TerminalPanel`). Cross-cutting helpers live in `state`.
- **Feature crates never depend on `ssh`/`local-shell`.** They create sessions through the `oneterm_terminal::SessionFactory` process-global that the `app` installs at startup.
- **Theme JSON** lives at `crates/theme/themes/<name>.json`, loaded via `BUILTIN_THEMES` in `crates/theme/src/theme.rs`. Do not hardcode colors — read from `cx.theme()` / `TerminalTheme`.
- **OneTerm icons**: drop an SVG into `crates/theme/assets/icons/<name>.svg`; `crates/theme/build.rs` + the `icon_named!` macro auto-generate the matching `AppIcon::<PascalName>` variant (see `crates/theme/src/icon.rs`).
- **Do not** put protocol logic (ssh, local) in any UI crate. UI crates only call through trait abstractions (`TerminalSession`, `SftpBackend`, `SessionFactory`).
- Shell detection (`resolve_shell`, `ShellKind`, `LocalShellConfig`) belongs in `core::config::shell`, **not** in the `local-shell` crate.

## 3. Responsibility of each crate (current state)

Layers, low → high. An arrow `A → B` means *A depends on B*.

| Crate (package) | Depends on | Layer | Responsibility |
|---|---|---|---|
| `core` (`oneterm-core`) | _(leaf)_ | domain | Error type, `SftpBackend`, `LocalShellConfig`/`ShellKind`, `SshConfig`/`SshAuthMethod`. No gpui, **no alacritty**. |
| `terminal` (`oneterm-terminal`) | `core` | engine | Terminal engine (alacritty-coupled, no gpui): `TerminalSession`, `SessionEvent`, `TerminalContent`, palette/OSC/key/mouse helpers, `PtySize` + `SessionFactory` trait + process global. |
| `highlight` (`oneterm-highlight`) | _(leaf)_ | engine | Semantic syntax-highlighting engine. |
| `actions` (`oneterm-actions`) | `core`, gpui | leaf-ui | gpui `Action` structs shared by shell + features. |
| `settings` (`oneterm-settings`) | `core`, gpui | shared | `TerminalConfig` (terminal.json), live `TerminalSettings`, `UiConfig` (ui_config.json). |
| `state` (`oneterm-state`) | `core`, `terminal`, gpui, gpui-component | shared | `AppState`, notif helpers, `WorkspaceCommands` registry, dock-walking + `set_right_dock_open`, active-terminal metrics provider, shared persistence paths. |
| `theme` (`oneterm-theme`) | `settings`, `actions`, gpui, gpui-component | shared | Theme registry + 24 built-in themes, `AppIcon` (generated from SVGs). |
| `workspace` (`oneterm-workspace`) | `core`, `terminal`, `settings`, `state`, `actions`, `theme` | shell | Feature-**agnostic** app shell: `OneTermWorkspace` (DockArea) + persistence, title bar, menus, statusbar + widgets. Builds feature panels **by name** via the gpui-component `PanelRegistry`; drives features via the `WorkspaceCommands` registry. **No feature/backend deps.** |
| `terminal-view` (`oneterm-terminal-view`) | `core`, `terminal`, `settings`, `state`, `theme`, `actions`, `highlight` | feature | Terminal panel + rendering + input + split spaces + terminal-settings panel. Registers `"terminal"`/`"terminal-settings"` panels. |
| `sftp-ui` (`oneterm-sftp-ui`) | `core`, `terminal`, `state`, `theme`, `actions` | feature | SFTP file browser + transfer queue. Registers `"sftp"`. |
| `session-ui` (`oneterm-session-ui`) | `core`, `terminal`, `terminal-view`, `state`, `actions` | feature | Session tree + connect dialogs + `SshSessionStore`. Registers `"session"`. |
| `settings-ui` (`oneterm-settings-ui`) | `core`, `settings`, `theme`, `actions` | feature | General Settings window (general/terminal/appearance/key-bindings/about). Provides `open_settings` + `setup_key_bindings` commands. |
| `ssh` (`oneterm-ssh`) | `core`, `terminal` (russh + russh-sftp) | backend | russh client + SFTP; `SshSession` (impl `TerminalSession`), `SftpSession` (impl `SftpBackend`). |
| `local-shell` (`oneterm-local-shell`) | `core`, `terminal` | backend | Local PTY (`alacritty_terminal::tty` + ConPTY); `LocalSession` (impl `TerminalSession`). |
| `app` (`oneterm-app`) | shell + all four features + `settings`/`theme`/`state` + `ssh` + `local-shell` | binary | Only crate that knows every layer. Installs `AppSessionFactory` (→ `ssh`/`local-shell`), runs `init()` (globals + feature `init()` + `WorkspaceCommands`), opens the window. Owns the `terminal-diagnostics` feature forwarding. |

## 3.1 Crate & dependency rules

The **hard crate & dependency rules** — the layer diagram, the invariants
**R1–R12** (no cycle, no UI→backend edge, feature-agnostic shell, no feature
cross-deps except `session-ui → terminal-view`, `core`/`terminal` stay
gpui/alacritty-free, …), and the one-shot `cargo tree` verification — live in a
dedicated file:

> 📐 **[`docs/agents/crate-dependency-rules.md`](crate-dependency-rules.md)** — read
> and obey it before touching any crate's dependencies.

Quick summary: dependencies point **down only** (L0 → L4), the graph is a DAG, the
shell (`workspace`) never depends on a feature or backend, and UI crates create
sessions via `oneterm_terminal::SessionFactory` instead of depending on
`ssh`/`local-shell`. See the doc for the full table and verification commands.

## 4. When adding a new crate / module

- Open an issue / TODO before adding a crate beyond those in §3.
- Crate package names use the `oneterm-<name>` form; the `core` re-export inside each backend crate is aliased as `core` (e.g. `use oneterm_core as core`).
- Path dependencies under the workspace directory are automatically workspace members; still list new crates in the root `members = [...]` for clarity.
- Add the crate to the dependency table in §3 and the tree in §1.
- **Respect the layering** (see the hard rules in [`crate-dependency-rules.md`](crate-dependency-rules.md)): a new shared type goes in the lowest crate that needs it (R10); a new feature is its own `*-ui` crate that depends on the shared layers only (R2, R5); keep `ssh`/`local-shell` out of every UI crate — route through `SessionFactory` (R3). After adding a crate, re-run that doc's "full-graph verification" commands.

## 5. Planned structure expansion (not yet created)

The items below **do not yet exist** on disk; they are recorded as a roadmap.

```
# To be added when needed
├── clippy.toml                     # Not yet (workspace lints currently live in Cargo.toml [workspace.lints])
├── assets/                         # Not yet at repo root — shared static assets
│   └── locales/{en.yml, vi.yml}    # i18n (rust-i18n)
│
└── crates/
    ├── session-ui/src/host_manager/   # Host management UI (host list currently lives in the session tree)
    └── (shared widgets: confirm_dialog / empty_state / toast — add under workspace/widgets when needed)
```

> ⚠️ When implementing parts of §5, **update §1** (move them from "planned" to "actual") and remove them from §5.
