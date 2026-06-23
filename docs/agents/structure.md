# Cấu trúc dự án — myTerm2

> File tách từ `AGENTS.md` (section 2). Mô tả cấu trúc workspace, cây thư mục chuẩn, và quy tắc tổ chức file Rust.
>
> 📌 Cây thư mục ở §1 phản ánh **trạng thái thực tế trên disk** (commit hiện tại). Các phần **kế hoạch** (chưa tạo) được liệt kê riêng ở §5 để làm roadmap.

## 1. Cây thư mục (trạng thái thực tế)

```
myTerm2/
├── Cargo.toml                      # Workspace root — members + workspace deps + lints
├── Cargo.lock
├── AGENTS.md                       # File entry point cho agent
├── .gitignore
├── .rustfmt.toml
│
├── crates/
│   ├── app/                        # Binary: main.rs + wiring
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs             # Entry point — init gpui-component + myterm2_ui, mở window
│   │       └── window.rs           # open_window(cx) — tạo MainWindow + gắn MyTermWorkspace
│   │
│   ├── core/                       # Domain model, business logic (no GPUI)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # Re-export: AppError, LocalShellConfig, ShellKind, TerminalSession...
│   │       ├── error.rs            # AppError (thiserror) + Result<T>
│   │       ├── config/             # Cấu hình terminal (shell cục bộ)
│   │       │   ├── mod.rs
│   │       │   └── shell.rs        # LocalShellConfig + ShellKind + resolve_shell (cmd/pwsh/COMSPEC/chcp)
│   │       └── terminal/           # Terminal rendering & input helpers (framework-agnostic)
│   │           ├── mod.rs          # Re-export tất cả submodule
│   │           ├── session.rs      # TerminalSession trait + SessionEvent + CursorBounds
│   │           ├── content.rs      # TerminalContent + IndexedCell + TerminalBounds (display iter)
│   │           ├── palette.rs      # TerminalPalette + resolve_color (ANSI 16/256/truecolor)
│   │           ├── colors_util.rs # is_default_background_color / is_decorative_character / is_app_chosen_exact_color
│   │           ├── key_encode.rs   # encode_key + KeySpec + NamedKey + KeyMods (keyboard input → ANSI)
│   │           ├── mouse_encode.rs # encode_mouse_press/release/move/wheel (mouse → ANSI)
│   │           ├── osc.rs         # OSC 52 (clipboard base64) + parse_cwd_url + OscSink
│   │           └── url.rs         # link_ranges + url_at (URL detection qua linkify)
│   │
│   ├── ssh/                        # Triển khai SSH + SFTP — PLACEHOLDER
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs              # Re-export myterm2_core as core (chưa có triển khai russh)
│   │
│   ├── local/                      # Local shell qua PTY (alacritty_terminal::tty + EventLoop/ConPTY)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # Re-export LocalSession + LocalListener + PtySize + core
│   │       ├── session.rs          # LocalSession struct + spawn + helpers + lifecycle (~175 dòng)
│   │       ├── session_terminal.rs # impl TerminalSession for LocalSession (~334 dòng)
│   │       ├── session_tests.rs    # Tests for LocalSession (~192 dòng, #[cfg(test)])
│   │       ├── listener.rs         # LocalListener: EventListener impl (forward → SessionEvent)
│   │       └── state.rs            # State chia sẻ cho local session
│   │
│   └── ui/                         # Toàn bộ GPUI + gpui-component
│       ├── Cargo.toml
│       ├── themes/                 # JSON theme built-in (24 theme: catppuccin, gruvbox, tokyonight, ...)
│       │   ├── adventure.json
│       │   ├── catppuccin.json
│       │   ├── gruvbox.json
│       │   ├── solarized.json
│       │   ├── tokyonight.json
│       │   ├── zed-one-dark.json
│       │   ├── zed-one-light.json
│       │   └── ... (xem đầy đủ trong folder)
│       └── src/
│           ├── lib.rs              # init(cx): theme + AppState + TerminalSettings + register_panel x4
│           ├── actions.rs          # UI-level actions (Zed action registration)
│           ├── theme.rs            # Theme registration + load built-in themes từ crates/ui/themes
│           │
│           ├── layout/             # Layout chính của app
│           │   ├── mod.rs
│           │   ├── workspace.rs     # MyTermWorkspace: DockArea tổng + bind_keys
│           │   ├── title_bar.rs     # Title bar (top)
│           │   ├── app_menus.rs    # Menu bar (File/Edit/View/...)
│           │   └── statusbar.rs    # Status bar (bottom)
│           │
│           ├── views/              # Các màn hình lớn (PanelView cho DockArea)
│           │   ├── mod.rs          # Re-export: SessionPanel, SftpPanel, TerminalPanel, TerminalSettingsPanel
│           │   ├── session_tabs/   # Tab quản lý session
│           │   │   ├── mod.rs
│           │   │   └── tabs.rs     # SessionPanel (dock panel)
│           │   ├── sftp/           # SFTP file browser
│           │   │   ├── mod.rs
│           │   │   └── file_browser.rs  # SftpPanel (dock panel, placeholder)
│           │   └── terminal/       # Terminal emulator view
│           │       ├── mod.rs              # Re-export panel/view/theme + handler modules
│           │       ├── terminal_view.rs    # LocalTerminalView struct + inherent helpers (~502 dòng)
│           │       ├── terminal_render.rs  # impl Render + Focusable for LocalTerminalView (~312 dòng)
│           │       ├── terminal_handlers.rs # Mouse/wheel/key/context-menu handlers (~751 dòng)
│           │       ├── terminal_input.rs   # Keyboard + vi-mode + scroll shortcuts (~480 dòng)
│           │       ├── terminal_mouse.rs   # Mouse/selection/wheel helpers (~261 dòng)
│           │       ├── terminal_ime.rs     # EntityInputHandler impl (~115 dòng)
│           │       ├── terminal_element.rs        # TerminalElement orchestration (prepain/paint) (~633 dòng)
│           │       ├── terminal_element_layout.rs # RowLayoutCache + update_row_cache + layout_selection
│           │       ├── terminal_element_cell.rs   # Per-cell color/text-run helpers
│           │       ├── terminal_element_box.rs  # Box-drawing / block / powerline primitives
│           │       ├── terminal_panel.rs          # TerminalPanel (PanelView dock)
│           │       ├── terminal_scrollbar.rs      # Scrollbar tuỳ chỉnh cho terminal
│           │       ├── terminal_settings_panel.rs # TerminalSettingsPanel (dock panel cài đặt)
│           │       └── theme.rs                 # TerminalTheme + build_terminal_theme + resolve_cell_color
│           │
│           ├── components/         # UI component tái sử dụng
│           │   ├── mod.rs
│           │   └── datetime_clock.rs  # Đồng hồ hiển thị trong statusbar
│           │
│           └── state/              # AppState chia sẻ — Entity<T> state toàn cục
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
├── docs/                           # Tài liệu phát triển
│   ├── gui-layout.md              # Thiết kế layout GUI
│   ├── terminal-backend.md       # Thiết kế terminal backend (local + ssh, render alacritty)
│   └── agents/                    # AGENTS files (tách nhỏ)
│       ├── code-style.md           # Quy ước code
│       ├── dependencies.md         # Deps + rev lock + reference-first
│       └── structure.md            # File này
│
└── reference/                      # Clone local của gpui-component (gitignored)
    └── gpui-component/
```

## 2. Quy tắc cấu trúc

- **Mỗi file Rust tối đa ~400 dòng.** Nếu vượt → tách module con ngay. Ngược lại, **không tách quá nhỏ** dẫn đến 1 file chỉ có 5–10 dòng. Mỗi file phải có đủ "khối lượng trách nhiệm" để tồn tại độc lập.
- **Một module, một trách nhiệm.** Tên file = tên module chính (snake_case).
- **Folder `views/<feature>/`** cho mỗi màn hình lớn: `mod.rs` (re-export + state), `<feature>_view.rs` (Render), `<feature>_panel.rs` (nếu cần dock), `<feature>_element.rs` (nếu custom element).
- **Folder `components/`** chỉ chứa widget thuần, không phụ thuộc domain.
- **Folder `state/`** chứa `Entity<T>` state toàn cục; UI chỉ đọc/ghi qua `cx.global::<AppState>()` hoặc `cx.entity::<T>()`.
- **Theme JSON** đặt tại `crates/ui/themes/<name>.json` (built-in), load qua `crates/ui/src/theme.rs`. Không hardcode màu trong component — đọc từ `cx.theme()` / `TerminalTheme`.
- **Không** đặt logic giao thức (ssh, local) trong crate `ui`. UI chỉ gọi qua trait abstraction (vd. `TerminalSession`).
- Shell detection (`resolve_shell`, `ShellKind`, `LocalShellConfig`) thuộc về `core::config::shell`, **không** đặt trong `local` crate.

## 3. Trách nhiệm từng crate (trạng thái hiện tại)

| Crate | Phụ thuộc | Trạng thái | Trách nhiệm |
|---|---|---|---|
| `app` | `ui`, `ssh`, `local`, `core` | ✅ Skeleton | Binary entry point. `main.rs` init gpui-component + myterm2_ui, mở window. `window.rs` gắn `MyTermWorkspace`. |
| `core` | _(không — leaf crate)_ | ✅ Đang triển khai | Domain types, `TerminalSession` trait, `SessionEvent`, `AppError`, `LocalShellConfig`/`ShellKind`, terminal helpers (content, palette, key/mouse encode, OSC, URL). Không phụ thuộc `gpui`. |
| `ssh` | `core` | ⬜ Placeholder | Sẽ triển khai `russh`: client, channel, SFTP, known_hosts, auth. Hiện chỉ re-export `core`. |
| `local` | `core` | ✅ Đang triển khai | PTY qua `alacritty_terminal::tty` + `EventLoop` (ConPTY trên Windows). `LocalSession` + `LocalListener`. Implement `TerminalSession`. Xem [`docs/terminal-backend.md`](../terminal-backend.md). |
| `ui` | `core` _(không `ssh`/`local`)_ | ✅ Đang triển khai | Toàn bộ gpui: `MyTermWorkspace` (DockArea), title bar, app menus, statusbar, terminal view/element/scrollbar, session tabs, SFTP panel, AppState, TerminalSettings, theme + 24 built-in themes. Giao tiếp `ssh`/`local` qua trait. |

> 🔗 **Quy tắc phụ thuộc**: `app → {ui, ssh, local, core}`, `ui → core`, `ssh → core`, `local → core`. Không có cycle, không peer-to-peer giữa `ssh` và `local`.

## 4. Khi thêm crate / module mới

- Mở issue / TODO trước khi thêm crate mới ngoài 5 crate trên.
- Tên crate dùng `snake_case`.
- Mỗi crate mới phải có `Cargo.toml` riêng và là thành viên của workspace (`members = [...]` trong `Cargo.toml` root).
- Thêm crate vào bảng phụ thuộc ở §3.
- Cập nhật cây thư mục ở §1.

## 5. Kế hoạch mở rộng cấu trúc (chưa tạo)

Các phần dưới đây **chưa tồn tại** trên disk, ghi lại để làm roadmap khi triển khai:

```
# Sẽ thêm khi cần
├── README.md                       # Chưa có
├── clippy.toml                     # Chưa có (workspace lints đang设在 Cargo.toml [workspace.lints])
├── config/                         # Chưa có — file cấu hình mặc định
│   ├── default.toml
│   └── themes/                      # (built-in themes hiện nằm trong crates/ui/themes/)
├── assets/                         # Chưa có — tài nguyên tĩnh
│   ├── icons/                      # SVG icon theo Lucide (đặt tên trùng IconName)
│   ├── fonts/
│   └── locales/
│       ├── en.yml                  # i18n (rust-i18n)
│       └── vi.yml
│
└── crates/
    ├── app/src/
    │   ├── app.rs                  # Application struct, global state (chưa tách riêng)
    │   └── actions.rs             # Global actions / key bindings (hiện gộp trong main.rs)
    │
    ├── ssh/src/                    # Triển khai russh
    │   ├── client.rs              # russh::client wrapper
    │   ├── channel.rs             # Shell channel + resize
    │   ├── sftp.rs                # SFTP subsystem
    │   ├── known_hosts.rs         # OpenSSH known_hosts parser
    │   └── auth.rs                # password / pubkey / agent
    │
    └── ui/src/
        ├── root.rs                # Root view wrapper (chưa tách)
        ├── icons.rs               # IconName constants
        ├── layout/
        │   └── sidebar.rs         # Sidebar host list (chưa có)
        ├── views/
        │   ├── host_manager/       # Quản lý host
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

> ⚠️ Khi triển khai các phần trong §5, **cập nhật §1** (chuyển từ "kế hoạch" sang "thực tế") và xoá khỏi §5.