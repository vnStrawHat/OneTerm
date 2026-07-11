# Project structure — OneTerm

> File split from `AGENTS.md` (section 2). Describes the workspace structure, the canonical directory tree, and the Rust file organization conventions.
>
> 📌 The tree in §1 reflects the **actual on-disk state** (current commit). **Planned** parts (not yet created) are listed separately in §5 as a roadmap.

## 1. Directory tree (actual state)

```
OneTerm/
├── Cargo.toml                      # Workspace root — members + workspace deps + lints
├── Cargo.lock
├── AGENTS.md                       # Entry-point file for the agent
├── README.md                       # Project README
├── VERSION                         # Single-source version string
├── .gitignore
├── .rustfmt.toml
│
├── crates/
│   ├── app/                        # Binary: lib + two bin shims + wiring
│   │   ├── Cargo.toml              # name = "oneterm-app", default-run = "oneterm-debug"
│   │   ├── build.rs                # Build script: embed app icon (.rc) + copy conpty.dll/OpenConsole.exe
│   │   ├── assets/                 # Runtime resources (Windows + macOS)
│   │   │   ├── oneterm.rc          # Resource script: app icon (numeric ID 1 = gpui window icon + Explorer default) + VS_VERSION_INFO
│   │   │   ├── conpty.dll         # ConPTY shim (alacritty_terminal LoadLibrary)
│   │   │   ├── x64/OpenConsole.exe # ConPTY host (Windows Terminal)
│   │   │   └── icons/             # App icon (multi-resolution, embedded into the exe)
│   │   │       ├── terminal-48x48.ico
│   │   │       └── terminal-96x96.ico
│   │   ├── macos/                 # macOS .app bundle resources
│   │   │   └── Info.plist         # Bundle descriptor template ({{VERSION}}); declares GUI app (NSPrincipalClass=NSApplication) so double-clicking in Finder doesn't open Terminal.app
│   │   └── src/
│   │       ├── lib.rs              # oneterm_app::run() — init app + UI, open window (shared by both bins)
│   │       ├── assets.rs           # CustomAssets (asset source for fonts/icons)
│   │       ├── window.rs           # open_window(cx) — create MainWindow + attach OneTermWorkspace
│   │       └── bin/
│   │           ├── oneterm.rs          # Release binary → oneterm(.exe) (WINDOWS subsystem, no console)
│   │           └── oneterm-debug.rs    # Dev binary → oneterm-debug(.exe) (keeps console for logs)
│   │
│   ├── core/                       # Domain model, business logic (no GPUI) — leaf crate
│   │   ├── Cargo.toml              # name = "oneterm-core"
│   │   └── src/
│   │       ├── lib.rs              # Re-export: AppError, LocalShellConfig, ShellKind, TerminalSession, SftpBackend...
│   │       ├── error.rs            # AppError (thiserror) + Result<T>
│   │       ├── sftp.rs             # SFTP abstraction: SftpBackend trait + FileEntry + FileStat
│   │       ├── config/             # Terminal configuration (local shell)
│   │       │   ├── mod.rs
│   │       │   └── shell.rs        # LocalShellConfig + ShellKind + resolve_shell (cmd/pwsh/COMSPEC/chcp)
│   │       └── terminal/           # Terminal rendering & input helpers (framework-agnostic)
│   │           ├── mod.rs          # Re-export all submodules
│   │           ├── session.rs      # TerminalSession trait + SessionEvent + CursorBounds + TerminalInfo
│   │           ├── content.rs      # TerminalContent + IndexedCell + TerminalBounds (display iter)
│   │           ├── palette.rs      # TerminalPalette + resolve_color (ANSI 16/256/truecolor)
│   │           ├── colors_util.rs # is_default_background_color / is_decorative_character / is_app_chosen_exact_color
│   │           ├── key_encode.rs   # encode_key + KeySpec + NamedKey + KeyMods (keyboard input → ANSI)
│   │           ├── mouse_encode.rs # encode_mouse_press/release/move/wheel (mouse → ANSI)
│   │           ├── osc.rs         # OSC 52 (clipboard base64) + parse_cwd_url + OscSink
│   │           ├── osc_color.rs   # OSC color sequences (foreground/background/current-color)
│   │           └── url.rs         # link_ranges + url_at (URL detection via linkify)
│   │
│   ├── ssh/                        # SSH + SFTP implementation (russh + hidden tokio runtime)
│   │   ├── Cargo.toml              # name = "oneterm-ssh"
│   │   └── src/
│   │       ├── lib.rs              # Re-export SshSession, SftpSession, SshConfig, SshListener + core
│   │       ├── config.rs           # SshConfig + SshAuthMethod (password/pubkey/agent)
│   │       ├── session.rs         # SshSession + connect + PtySize (russh client + shell channel)
│   │       ├── session_terminal.rs # impl TerminalSession for SshSession
│   │       ├── handler.rs         # russh::client::Handler impl
│   │       ├── listener.rs        # SshListener + Cmd (forward → SessionEvent)
│   │       ├── sftp.rs            # SftpSession + SftpCmd + SftpEvent (russh-sftp subsystem)
│   │       ├── sftp_task.rs       # SFTP background task / polling
│   │       ├── task.rs            # Generic async task helpers
│   │       ├── counting_stream.rs # Counting stream (bandwidth accounting, re-exported as core::NetStats)
│   │       └── state.rs           # Shared state for an SSH session
│   │
│   ├── local/                      # Local shell over PTY (alacritty_terminal::tty + EventLoop/ConPTY)
│   │   ├── Cargo.toml              # name = "oneterm-local"
│   │   └── src/
│   │       ├── lib.rs              # Re-export LocalSession + LocalListener + PtySize + core
│   │       ├── session.rs          # LocalSession struct + spawn + helpers + lifecycle
│   │       ├── session_terminal.rs # impl TerminalSession for LocalSession
│   │       ├── session_tests.rs    # Tests for LocalSession (#[cfg(test)])
│   │       ├── listener.rs         # LocalListener: EventListener impl (forward → SessionEvent)
│   │       ├── event_loop.rs      # alacritty_terminal EventLoop wiring (ConPTY on Windows)
│   │       └── state.rs            # Shared state for the local session
│   │
│   └── ui/                         # All GPUI + gpui-component
│       ├── Cargo.toml              # name = "oneterm-ui"
│       ├── build.rs                # Sets ONETERM_UI_ICONS_DIR env for the icon_named! macro
│       ├── assets/
│       │   └── icons/             # OneTerm SVG icons (auto-generate AppIcon variants via build.rs)
│       │       ├── file.svg, folder.svg, folder-sync.svg, refresh.svg, terminal.svg
│       ├── themes/                 # Built-in JSON themes (24 total: 2 Zed + 22 from gpui-component collection)
│       │   ├── zed-one-dark.json, zed-one-light.json
│       │   └── … (adventure, alduin, asciinema, aurora, ayu, catppuccin, everforest, fahrenheit,
│       │        flexoki, gruvbox, harper, hybrid, jellybeans, kibble, macos-classic, matrix,
│       │        mellifluous, molokai, solarized, spaceduck, tokyonight, twilight)
│       └── src/
│           ├── lib.rs              # init(cx): theme + AppState + TerminalSettings + register_panel x4
│           ├── actions.rs          # UI-level actions (Zed action registration)
│           ├── icon.rs             # AppIcon enum (generated from assets/icons/*.svg) + RenderOnce
│           ├── notif_ext.rs        # Notification helpers / extensions
│           ├── theme.rs            # Theme registration + BUILTIN_THEMES list (load from crates/ui/themes)
│           │
│           ├── layout/             # Main app layout
│           │   ├── mod.rs
│           │   ├── title_bar.rs     # Title bar (top)
│           │   ├── app_menus.rs    # Menu bar (File/Edit/View/...)
│           │   ├── statusbar.rs    # Status bar (bottom)
│           │   └── workspace/      # OneTermWorkspace (DockArea overall)
│           │       ├── mod.rs          # OneTermWorkspace struct + bind_keys
│           │       ├── actions.rs     # Workspace-level actions
│           │       ├── layout.rs      # DockArea layout construction
│           │       ├── persistence.rs # Save/restore dock layout (docks.json)
│           │       └── zoom.rs        # Zoom / font-size actions
│           │
│           ├── views/              # Major screens (PanelView for the DockArea)
│           │   ├── mod.rs          # Re-export: SessionPanel, SettingsPanel, SftpPanel, TerminalPanel
│           │   ├── session_tabs/   # Session management tabs
│           │   │   ├── mod.rs              # Re-export SessionPanel
│           │   │   ├── panel.rs            # SessionPanel struct + constructor + Panel/Focusable impls
│           │   │   ├── render.rs           # impl Render — header/empty/no-results/final div
│           │   │   ├── tree_render.rs      # Tree widget rendering — item renderer + context menu
│           │   │   ├── tree_builder.rs     # build_tree_items + session_matches + helpers
│           │   │   ├── connect_dialog.rs   # SSH connect dialog
│           │   │   ├── group_combo.rs      # GroupComboDelegate + group_combobox widget
│           │   │   ├── session_dialog.rs   # open_session_dialog + field helper
│           │   │   └── rename_group.rs     # open_rename_group_dialog
│           │   ├── sftp/           # SFTP file browser (uses core::SftpBackend)
│           │   │   ├── mod.rs              # Re-export SftpPanel
│           │   │   ├── types.rs            # Sort/transfer types + format helpers + column defs
│           │   │   ├── panel.rs            # SftpPanel struct + constructor + nav + Panel/Focusable impls
│           │   │   ├── actions.rs          # Rename/delete/new-folder/properties dialogs
│           │   │   ├── transfer.rs         # Upload/download with progress polling
│           │   │   ├── persistence.rs      # SFTP view state / breadcrumb persistence
│           │   │   ├── table_delegate.rs   # TableDelegate for the file list
│           │   │   ├── render.rs           # impl Render + breadcrumb/toolbar/column-headers/file-list
│           │   │   └── render_transfer.rs  # Transfer queue rendering + clear
│           │   ├── settings/       # General Settings UI (opens in a separate window; font, theme, key bindings, terminal, about)
│           │   │   ├── mod.rs              # Re-export SettingsPanel + open_settings_window
│           │   │   ├── panel.rs            # SettingsPanel (Render view wrapped in Root by window.rs)
│           │   │   ├── window.rs           # open_settings_window — standalone WindowHandle<Root>
│           │   │   ├── general.rs          # General page — UI font size + configurable key bindings group
│           │   │   ├── key_bindings.rs      # Configurable key bindings (press-to-rebind + reset; persists to ui_config.json)
│           │   │   ├── terminal.rs         # Terminal page — shell/font groups + page assembly + persist()
│           │   │   ├── terminal_options.rs  # Terminal page — cursor/layout/scroll/bell/security groups
│           │   │   ├── appearance.rs       # Appearance page — theme mode + theme list
│           │   │   └── about.rs            # About page — version + links
│           │   └── terminal/       # Terminal emulator view (split into themed submodules)
│           │       ├── mod.rs              # Re-export panel/view + submodules
│           │       ├── panel.rs            # TerminalPanel (PanelView dock)
│           │       ├── scrollbar.rs        # Custom scrollbar for the terminal
│           │       ├── settings_panel.rs   # TerminalSettingsPanel (settings dock panel)
│           │       ├── ime.rs              # EntityInputHandler impl
│           │       ├── url.rs              # URL detection helpers for the terminal view
│           │       ├── view/               # LocalTerminalView struct + inherent helpers
│           │       │   ├── mod.rs          # LocalTerminalView struct + constructor
│           │       │   ├── cursor.rs       # Cursor rendering / blink
│           │       │   ├── font.rs         # Font metrics / sizing
│           │       │   ├── grid.rs         # Grid (rows/cols) helpers
│           │       │   ├── key.rs          # Keyboard input dispatch
│           │       │   └── scrollbar.rs     # Scrollbar wiring for the view
│           │       ├── render/             # impl Render + Focusable + overlay painting
│           │       │   ├── mod.rs
│           │       │   ├── overlays.rs     # Link hover / IME / cursor overlays
│           │       │   └── theme_apply.rs  # Apply TerminalTheme to cells during paint
│           │       ├── element/            # TerminalElement orchestration (prepaint/paint)
│           │       │   ├── mod.rs
│           │       │   ├── prepaint.rs     # Layout pass
│           │       │   ├── paint.rs        # Paint pass
│           │       │   ├── measure.rs      # Text measurement
│           │       │   └── gutter.rs        # Gutter / margin rendering
│           │       ├── layout/             # Row layout cache + selection
│           │       │   ├── mod.rs
│           │       │   ├── cache.rs        # RowLayoutCache + update_row_cache
│           │       │   ├── row.rs          # Row layout
│           │       │   ├── selection.rs     # layout_selection
│           │       │   └── types.rs        # Layout types
│           │       ├── cell/               # Per-cell color/text-run helpers
│           │       │   ├── mod.rs
│           │       │   ├── batch.rs        # Text-run batching
│           │       │   ├── blank.rs        # Blank cell rendering
│           │       │   ├── color.rs        # Per-cell color resolution
│           │       │   ├── hash.rs         # Cell hashing
│           │       │   └── style.rs        # Cell style flags
│           │       ├── box_drawing/        # Box-drawing / block / powerline primitives
│           │       │   ├── mod.rs
│           │       │   ├── block.rs
│           │       │   ├── drawing.rs
│           │       │   ├── powerline.rs
│           │       │   ├── rounded.rs
│           │       │   └── shade.rs
│           │       ├── handlers/           # Mouse/wheel/key/context-menu/vi handlers
│           │       │   ├── mod.rs
│           │       │   ├── keyboard.rs     # Keyboard input
│           │       │   ├── mouse.rs        # Mouse / selection / wheel
│           │       │   ├── scroll.rs       # Scroll shortcuts
│           │       │   ├── vi.rs          # Vi-mode
│           │       │   ├── menu.rs         # Context menu
│           │       │   └── url.rs          # URL click / hover
│           │       └── theme/              # TerminalTheme + color resolution
│           │           ├── mod.rs          # TerminalTheme + build_terminal_theme
│           │           ├── palette.rs      # Palette mapping
│           │           ├── contrast.rs     # Contrast / default-color detection
│           │           ├── resolve.rs      # resolve_cell_color
│           │           └── tests.rs        # Theme tests
│           │
│           ├── components/         # Reusable UI components
│           │   ├── mod.rs
│           │   ├── datetime_clock.rs  # Clock displayed in the statusbar
│           │   ├── net_speed.rs       # Network speed indicator (statusbar)
│           │   └── resource.rs        # Resource (CPU/mem) indicator (statusbar)
│           │
│           └── state/              # Shared AppState — global Entity<T> state
│               ├── mod.rs
│               ├── app_state.rs    # AppState (init global)
│               ├── session_state.rs # Per-session UI state
│               ├── terminal_config/      # Terminal config JSON load/save
│               │   ├── mod.rs            # TerminalConfig + load/save
│               │   ├── font.rs           # FontConfig
│               │   ├── cursor.rs         # CursorConfig
│               │   ├── layout.rs         # LayoutConfig + PaddingConfig
│               │   ├── scroll.rs         # ScrollConfig
│               │   ├── bell.rs           # BellConfig
│               │   ├── colors.rs         # ColorsConfig
│               │   └── security.rs       # SecurityConfig (trusted host opts, etc.)
│               ├── ui_config.rs        # UiConfig (ui_font_size, theme_name, key_bindings) → ui_config.json
│               └── terminal_settings/    # Live TerminalSettings + mutators applied to sessions
│                   ├── mod.rs            # TerminalSettings
│                   ├── apply.rs         # Apply config → settings (config → live)
│                   ├── persist.rs       # Reverse: settings → config + save() (write terminal.json)
│                   ├── font.rs           # Font defaults + weight parsing
│                   ├── color.rs         # Hex color parse/serialize (parse_hex_color + hsla_to_hex)
│                   └── mutators.rs       # Mutator helpers
│
├── docs/                           # Development documentation
│   ├── gui-layout.md              # GUI layout design
│   ├── terminal-backend.md       # Terminal backend design (local + ssh, alacritty render)
│   ├── ssh-client-connect.md     # SSH client connection / auth design
│   ├── sftp-browser-design.md    # SFTP file browser design
│   ├── sftp-follow-terminal-cwd.md  # SFTP-follows-terminal-CWD design
│   ├── sftp-follow-terminal-cwd/  # Supporting notes for the SFTP-CWD feature
│   ├── osc-sequences-checklist.md # OSC sequence support checklist
│   ├── terminal-rendering-optimization.md # Terminal rendering optimization notes
│   ├── terminal-gap-analysis.md  # Terminal feature gap analysis
│   └── agents/                    # AGENTS files (split into smaller ones)
│       ├── code-style.md           # Code conventions
│       ├── dependencies.md         # Deps + rev lock + reference-first
│       └── structure.md            # This file
│
└── reference/                      # Local clone of gpui-component (gitignored)
    └── gpui-component/
```

## 2. Structure conventions

- **Each Rust file is at most ~400 lines.** If it exceeds that → split into a submodule immediately. Conversely, **do not split too small** such that a file ends up with only 5–10 lines. Each file must have enough "responsibility mass" to stand on its own.
- **One module, one responsibility.** The file name = the main module name (snake_case).
- **Folder `views/<feature>/`** for each major screen: `mod.rs` (re-export + state), a `view/` subfolder (struct + inherent helpers), a `render/` subfolder (`impl Render` + overlays), an `element/` subfolder (if a custom element is needed), plus `panel.rs` (if a dock is needed) and `handlers/` (input handlers). The terminal view is the reference shape for this layout.
- **Folder `components/`** holds only pure widgets, with no domain dependency.
- **Folder `state/`** holds global `Entity<T>` state; the UI only reads/writes it via `cx.global::<AppState>()` or `cx.entity::<T>()`.
- **Theme JSON** lives at `crates/ui/themes/<name>.json` (built-in), loaded via the `BUILTIN_THEMES` list in `crates/ui/src/theme.rs`. Do not hardcode colors in a component — read from `cx.theme()` / `TerminalTheme`.
- **OneTerm icons**: drop an SVG into `crates/ui/assets/icons/<name>.svg`; `build.rs` + the `icon_named!` macro auto-generate the matching `AppIcon::<PascalName>` variant (see `crates/ui/src/icon.rs`).
- **Do not** put protocol logic (ssh, local) in the `ui` crate. The UI only calls through trait abstractions (`TerminalSession`, `SftpBackend`).
- Shell detection (`resolve_shell`, `ShellKind`, `LocalShellConfig`) belongs in `core::config::shell`, **not** in the `local` crate.

## 3. Responsibility of each crate (current state)

| Crate (package) | Depends on | Status | Responsibility |
|---|---|---|---|
| `app` (`oneterm-app`) | `ui`, `core` _(+ `gpui-component-assets`)_ | ✅ In use | Binary entry. `lib.rs` exposes `run()` (init app + UI, open window); `bin/oneterm.rs` (release, WINDOWS subsystem) and `bin/oneterm-debug.rs` (dev, console for logs) are thin shims calling `run()`. `build.rs` embeds the app icon + copies ConPTY assets. `window.rs` attaches `OneTermWorkspace`. |
| `core` (`oneterm-core`) | _(none — leaf crate)_ | ✅ In use | Domain types & traits: `TerminalSession`, `SessionEvent`, `CursorBounds`/`TerminalInfo`, `AppError`, `LocalShellConfig`/`ShellKind`, SFTP abstraction (`SftpBackend`, `FileEntry`, `FileStat`), terminal helpers (content, palette, key/mouse encode, OSC + OSC color, URL). Does not depend on `gpui`. |
| `ssh` (`oneterm-ssh`) | `core` _(russh + russh-sftp)_ | ✅ In use | `russh` client with a hidden tokio runtime. `SshSession` (shell channel + `TerminalSession` impl), `SftpSession` (SFTP subsystem, implements `core::SftpBackend`), `SshConfig`/`SshAuthMethod` (password/pubkey/agent), `SshListener`, known-host handling, bandwidth accounting. See [`docs/terminal-backend.md`](../terminal-backend.md) and [`docs/ssh-client-connect.md`](../ssh-client-connect.md). |
| `local` (`oneterm-local`) | `core` | ✅ In use | PTY via `alacritty_terminal::tty` + `EventLoop` (ConPTY on Windows). `LocalSession` + `LocalListener`. Implements `TerminalSession`. See [`docs/terminal-backend.md`](../terminal-backend.md). |
| `ui` (`oneterm-ui`) | `core` _(not `ssh`/`local`)_ + `gpui-component-assets` | ✅ In use | All gpui: `OneTermWorkspace` (DockArea) + persistence, title bar, app menus, statusbar, terminal view/element/scrollbar/settings panel (split into `view`/`render`/`element`/`layout`/`cell`/`box_drawing`/`handlers`/`theme` submodules), session tabs, SFTP panel, `AppState`, `TerminalSettings` + `terminal_config`, `AppIcon` (auto-generated from SVGs), 24 built-in themes. Talks to `ssh`/`local` via traits (`TerminalSession`, `SftpBackend`). |

> 🔗 **Dependency rule**: `app → {ui, core}`, `ui → core`, `ssh → core`, `local → core`. No cycles, no peer-to-peer between `ssh` and `local`. (`app` does not depend on `ssh`/`local` directly — it only sees them through the `core` traits via `ui`.)

## 4. When adding a new crate / module

- Open an issue / TODO before adding a crate beyond the 5 above.
- Crate package names use the `oneterm-<name>` form (`oneterm-core`, `oneterm-ui`, …); the `core` re-export inside each crate is aliased as `core` (e.g. `use oneterm_core as core`).
- Each new crate must have its own `Cargo.toml` and be a workspace member (`members = [...]` in the root `Cargo.toml`).
- Add the crate to the dependency table in §3.
- Update the directory tree in §1.

## 5. Planned structure expansion (not yet created)

The items below **do not yet exist** on disk; they are recorded as a roadmap for implementation. (Items already realized have been moved into §1 and removed from here.)

```
# To be added when needed
├── clippy.toml                     # Not yet (workspace lints currently live in Cargo.toml [workspace.lints])
├── config/                         # Not yet — default config files
│   ├── default.toml
│   └── themes/                      # (built-in themes currently live in crates/ui/themes/)
├── assets/                         # Not yet at repo root — shared static assets (fonts/locales)
│   ├── fonts/
│   └── locales/
│       ├── en.yml                  # i18n (rust-i18n)
│       └── vi.yml
│
└── crates/
    └── ui/src/
        ├── root.rs                # Root view wrapper (not yet split out)
        ├── layout/
        │   └── sidebar.rs         # Host list sidebar (not yet)
        ├── views/
        │   ├── host_manager/       # Host management UI (not yet — host list currently lives in session_tabs)
        │   │   ├── mod.rs
        │   │   ├── host_list.rs
        │   │   ├── host_form.rs
        │   │   └── host_card.rs
        └── components/
            ├── confirm_dialog.rs   # Confirm dialog (not yet)
            ├── empty_state.rs      # Empty-state widget (not yet)
            └── toast.rs            # Toast wrapper (not yet)
```

> ⚠️ When implementing parts of §5, **update §1** (move them from "planned" to "actual") and remove them from §5.