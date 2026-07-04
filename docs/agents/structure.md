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
├── .gitignore
├── .rustfmt.toml
│
├── crates/
│   ├── app/                        # Binary: main.rs + wiring
│   │   ├── Cargo.toml
│   │   ├── build.rs             # Build script: embed app icon (.rc) + copy conpty.dll/OpenConsole.exe
│   │   ├── assets/              # Runtime resources (Windows)
│   │   │   ├── oneterm.rc        # Resource script: app icon (48+96) + VS_VERSION_INFO
│   │   │   ├── conpty.dll       # ConPTY shim (alacritty_terminal LoadLibrary)
│   │   │   ├── x64/OpenConsole.exe  # ConPTY host (Windows Terminal)
│   │   │   └── icons/           # App icon (multi-resolution, embedded into the exe)
│   │   │       ├── terminal-48x48.ico
│   │   │       └── terminal-96x96.ico
│   │   └── src/
│   │       ├── main.rs             # Entry point — init gpui-component + oneterm_ui, open window
│   │       └── window.rs           # open_window(cx) — create MainWindow + attach OneTermWorkspace
│   │
│   ├── core/                       # Domain model, business logic (no GPUI)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # Re-export: AppError, LocalShellConfig, ShellKind, TerminalSession...
│   │       ├── error.rs            # AppError (thiserror) + Result<T>
│   │       ├── config/             # Terminal configuration (local shell)
│   │       │   ├── mod.rs
│   │       │   └── shell.rs        # LocalShellConfig + ShellKind + resolve_shell (cmd/pwsh/COMSPEC/chcp)
│   │       └── terminal/           # Terminal rendering & input helpers (framework-agnostic)
│   │           ├── mod.rs          # Re-export all submodules
│   │           ├── session.rs      # TerminalSession trait + SessionEvent + CursorBounds
│   │           ├── content.rs      # TerminalContent + IndexedCell + TerminalBounds (display iter)
│   │           ├── palette.rs      # TerminalPalette + resolve_color (ANSI 16/256/truecolor)
│   │           ├── colors_util.rs # is_default_background_color / is_decorative_character / is_app_chosen_exact_color
│   │           ├── key_encode.rs   # encode_key + KeySpec + NamedKey + KeyMods (keyboard input → ANSI)
│   │           ├── mouse_encode.rs # encode_mouse_press/release/move/wheel (mouse → ANSI)
│   │           ├── osc.rs         # OSC 52 (clipboard base64) + parse_cwd_url + OscSink
│   │           └── url.rs         # link_ranges + url_at (URL detection via linkify)
│   │
│   ├── ssh/                        # SSH + SFTP implementation — PLACEHOLDER
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs              # Re-export oneterm_core as core (no russh implementation yet)
│   │
│   ├── local/                      # Local shell over PTY (alacritty_terminal::tty + EventLoop/ConPTY)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # Re-export LocalSession + LocalListener + PtySize + core
│   │       ├── session.rs          # LocalSession struct + spawn + helpers + lifecycle (~175 lines)
│   │       ├── session_terminal.rs # impl TerminalSession for LocalSession (~334 lines)
│   │       ├── session_tests.rs    # Tests for LocalSession (~192 lines, #[cfg(test)])
│   │       ├── listener.rs         # LocalListener: EventListener impl (forward → SessionEvent)
│   │       └── state.rs            # Shared state for the local session
│   │
│   └── ui/                         # All GPUI + gpui-component
│       ├── Cargo.toml
│       ├── themes/                 # Built-in JSON themes (2 themes: Zed One Dark, Zed One Light)
│       │   ├── zed-one-dark.json
│       │   └── zed-one-light.json
│       └── src/
│           ├── lib.rs              # init(cx): theme + AppState + TerminalSettings + register_panel x4
│           ├── actions.rs          # UI-level actions (Zed action registration)
│           ├── theme.rs            # Theme registration + load built-in themes from crates/ui/themes
│           │
│           ├── layout/             # Main app layout
│           │   ├── mod.rs
│           │   ├── workspace.rs     # OneTermWorkspace: DockArea overall + bind_keys
│           │   ├── title_bar.rs     # Title bar (top)
│           │   ├── app_menus.rs    # Menu bar (File/Edit/View/...)
│           │   └── statusbar.rs    # Status bar (bottom)
│           │
│           ├── views/              # Major screens (PanelView for the DockArea)
│           │   ├── mod.rs          # Re-export: SessionPanel, SftpPanel, TerminalPanel, TerminalSettingsPanel
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
│           │   ├── sftp/           # SFTP file browser
│           │   │   ├── mod.rs              # Re-export SftpPanel
│           │   │   ├── types.rs            # Sort/transfer types + format helpers + column defs
│           │   │   ├── panel.rs            # SftpPanel struct + constructor + nav + Panel/Focusable impls
│           │   │   ├── actions.rs          # Rename/delete/new-folder/properties dialogs
│           │   │   ├── transfer.rs         # Upload/download with progress polling
│           │   │   ├── render.rs           # impl Render + breadcrumb/toolbar/column-headers/file-list
│           │   │   ├── render_list.rs      # Entry row rendering + context menu
│           │   │   └── render_transfer.rs  # Transfer queue rendering + clear
│           │   └── terminal/       # Terminal emulator view
│           │       ├── mod.rs              # Re-export panel/view/theme + handler modules
│           │       ├── terminal_view.rs    # LocalTerminalView struct + inherent helpers (~502 lines)
│           │       ├── terminal_render.rs  # impl Render + Focusable for LocalTerminalView (~312 lines)
│           │       ├── terminal_handlers.rs # Mouse/wheel/key/context-menu handlers (~751 lines)
│           │       ├── terminal_input.rs   # Keyboard + vi-mode + scroll shortcuts (~480 lines)
│           │       ├── terminal_mouse.rs   # Mouse/selection/wheel helpers (~261 lines)
│           │       ├── terminal_ime.rs     # EntityInputHandler impl (~115 lines)
│           │       ├── terminal_element.rs        # TerminalElement orchestration (prepain/paint) (~633 lines)
│           │       ├── terminal_element_layout.rs # RowLayoutCache + update_row_cache + layout_selection
│           │       ├── terminal_element_cell.rs   # Per-cell color/text-run helpers
│           │       ├── terminal_element_box.rs  # Box-drawing / block / powerline primitives
│           │       ├── terminal_panel.rs          # TerminalPanel (PanelView dock)
│           │       ├── terminal_scrollbar.rs      # Custom scrollbar for the terminal
│           │       ├── terminal_settings_panel.rs # TerminalSettingsPanel (settings dock panel)
│           │       └── theme.rs                 # TerminalTheme + build_terminal_theme + resolve_cell_color
│           │
│           ├── components/         # Reusable UI components
│           │   ├── mod.rs
│           │   └── datetime_clock.rs  # Clock displayed in the statusbar
│           │
│           └── state/              # Shared AppState — global Entity<T> state
│               ├── mod.rs
│               ├── app_state.rs    # AppState (init global)
│               ├── terminal_settings.rs  # TerminalSettings (font, scrollback, ...)
│               └── terminal_config/      # Terminal config JSON load/save
│                   ├── mod.rs            # TerminalConfig + load/save + tests
│                   ├── font.rs           # FontConfig
│                   ├── cursor.rs         # CursorConfig
│                   ├── layout.rs         # LayoutConfig + PaddingConfig
│                   ├── scroll.rs         # ScrollConfig
│                   ├── bell.rs           # BellConfig
│                   └── colors.rs         # ColorsConfig
│
├── docs/                           # Development documentation
│   ├── gui-layout.md              # GUI layout design
│   ├── terminal-backend.md       # Terminal backend design (local + ssh, alacritty render)
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
- **Folder `views/<feature>/`** for each major screen: `mod.rs` (re-export + state), `<feature>_view.rs` (Render), `<feature>_panel.rs` (if a dock is needed), `<feature>_element.rs` (if a custom element is needed).
- **Folder `components/`** holds only pure widgets, with no domain dependency.
- **Folder `state/`** holds global `Entity<T>` state; the UI only reads/writes it via `cx.global::<AppState>()` or `cx.entity::<T>()`.
- **Theme JSON** lives at `crates/ui/themes/<name>.json` (built-in), loaded via `crates/ui/src/theme.rs`. Do not hardcode colors in a component — read from `cx.theme()` / `TerminalTheme`.
- **Do not** put protocol logic (ssh, local) in the `ui` crate. The UI only calls through trait abstractions (e.g. `TerminalSession`).
- Shell detection (`resolve_shell`, `ShellKind`, `LocalShellConfig`) belongs in `core::config::shell`, **not** in the `local` crate.

## 3. Responsibility of each crate (current state)

| Crate | Depends on | Status | Responsibility |
|---|---|---|---|
| `app` | `ui`, `ssh`, `local`, `core` | ✅ Skeleton | Binary entry point. `main.rs` inits gpui-component + oneterm_ui, opens a window. `window.rs` attaches `OneTermWorkspace`. |
| `core` | _(none — leaf crate)_ | ✅ In progress | Domain types, `TerminalSession` trait, `SessionEvent`, `AppError`, `LocalShellConfig`/`ShellKind`, terminal helpers (content, palette, key/mouse encode, OSC, URL). Does not depend on `gpui`. |
| `ssh` | `core` | ⬜ Placeholder | Will implement `russh`: client, channel, SFTP, known_hosts, auth. Currently only re-exports `core`. |
| `local` | `core` | ✅ In progress | PTY via `alacritty_terminal::tty` + `EventLoop` (ConPTY on Windows). `LocalSession` + `LocalListener`. Implements `TerminalSession`. See [`docs/terminal-backend.md`](../terminal-backend.md). |
| `ui` | `core` _(not `ssh`/`local`)_ | ✅ In progress | All gpui: `OneTermWorkspace` (DockArea), title bar, app menus, statusbar, terminal view/element/scrollbar, session tabs, SFTP panel, AppState, TerminalSettings, theme + 2 built-in themes (Zed One Dark/Light). Talks to `ssh`/`local` via traits. |

> 🔗 **Dependency rule**: `app → {ui, ssh, local, core}`, `ui → core`, `ssh → core`, `local → core`. No cycles, no peer-to-peer between `ssh` and `local`.

## 4. When adding a new crate / module

- Open an issue / TODO before adding a crate beyond the 5 above.
- Crate names use `snake_case`.
- Each new crate must have its own `Cargo.toml` and be a workspace member (`members = [...]` in the root `Cargo.toml`).
- Add the crate to the dependency table in §3.
- Update the directory tree in §1.

## 5. Planned structure expansion (not yet created)

The sections below **do not yet exist** on disk; they are recorded as a roadmap for implementation:

```
# To be added when needed
├── README.md                       # Not yet
├── clippy.toml                     # Not yet (workspace lints live in Cargo.toml [workspace.lints])
├── config/                         # Not yet — default config files
│   ├── default.toml
│   └── themes/                      # (built-in themes currently live in crates/ui/themes/)
├── assets/                         # Not yet — static assets
│   ├── icons/                      # Lucide SVG icons (named to match IconName)
│   ├── fonts/
│   └── locales/
│       ├── en.yml                  # i18n (rust-i18n)
│       └── vi.yml
│
└── crates/
    ├── app/src/
    │   ├── app.rs                  # Application struct, global state (not yet split out)
    │   └── actions.rs             # Global actions / key bindings (currently merged into main.rs)
    │
    ├── ssh/src/                    # russh implementation
    │   ├── client.rs              # russh::client wrapper
    │   ├── channel.rs             # Shell channel + resize
    │   ├── sftp.rs                # SFTP subsystem
    │   ├── known_hosts.rs         # OpenSSH known_hosts parser
    │   └── auth.rs                # password / pubkey / agent
    │
    └── ui/src/
        ├── root.rs                # Root view wrapper (not yet split out)
        ├── icons.rs               # IconName constants
        ├── layout/
        │   └── sidebar.rs         # Host list sidebar (not yet)
        ├── views/
        │   ├── host_manager/       # Host management
        │   │   ├── mod.rs
        │   │   ├── host_list.rs
        │   │   ├── host_form.rs
        │   │   └── host_card.rs
        │   └── settings/          # Settings UI
        │       ├── mod.rs
        │       ├── general.rs
        │       ├── terminal.rs
        │       ├── appearance.rs
        │       └── about.rs
        ├── components/
        │   ├── connect_dialog.rs
        │   ├── confirm_dialog.rs
        │   ├── empty_state.rs
        │   └── toast.rs
        └── state/
            ├── session_state.rs
            └── ui_state.rs
```

> ⚠️ When implementing parts of §5, **update §1** (move them from "planned" to "actual") and remove them from §5.