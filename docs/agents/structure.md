# Cấu trúc dự án — myTerm2

> File tách từ `AGENTS.md` (section 2). Mô tả cấu trúc workspace, cây thư mục chuẩn, và quy tắc tổ chức file Rust.

## 1. Cây thư mục (bắt buộc tuân thủ)

```
myTerm2/
├── Cargo.toml                      # Workspace root
├── Cargo.lock
├── AGENTS.md                       # File entry point cho agent
├── README.md
├── .gitignore
├── .rustfmt.toml
├── clippy.toml
│
├── config/                         # File cấu hình mặc định
│   ├── default.toml
│   └── themes/
│       ├── dark.json
│       └── light.json
│
├── assets/                         # Tài nguyên tĩnh
│   ├── icons/                      # SVG icon theo Lucide (đặt tên trùng IconName)
│   ├── fonts/
│   └── locales/
│       ├── en.yml
│       └── vi.yml
│
├── crates/
│   ├── app/                        # Binary: main.rs + wiring
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs             # Entry point — chỉ gọi run_app()
│   │       ├── app.rs              # Application struct, global state
│   │       ├── window.rs           # Mở window, gắn Root
│   │       └── actions.rs          # Global actions / key bindings
│   │
│   ├── core/                       # Domain model, business logic (no GPUI)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs            # AppError + thiserror
│   │       ├── connection/         # Host, credentials, protocol enum
│   │       │   ├── mod.rs
│   │       │   ├── host.rs
│   │       │   ├── credentials.rs
│   │       │   └── protocol.rs
│   │       ├── session/            # SessionId, SessionMeta
│   │       │   ├── mod.rs
│   │       │   ├── id.rs
│   │       │   └── meta.rs
│   │       ├── terminal/           # Grid, cell, parser façade
│   │       │   ├── mod.rs
│   │       │   ├── grid.rs
│   │       │   ├── cell.rs
│   │       │   └── parser.rs
│   │       └── config/             # Settings types
│   │           ├── mod.rs
│   │           ├── settings.rs
│   │           └── store.rs        # Load/save vào XDG / AppData
│   │
│   ├── ssh/                        # Triển khai SSH + SFTP
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # Re-export + connect() entry
│   │       ├── client.rs           # russh::client wrapper
│   │       ├── channel.rs          # Shell channel + resize
│   │       ├── sftp.rs             # SFTP subsystem
│   │       ├── known_hosts.rs      # OpenSSH known_hosts parser
│   │       └── auth.rs             # password / pubkey / agent
│   │
│   ├── local/                      # Local shell qua PTY
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── shell.rs            # portable-pty wrapper
│   │
│   └── ui/                         # Toàn bộ GPUI + gpui-component
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs              # Re-exports
│           ├── root.rs             # Root view wrapper
│           ├── theme.rs            # Theme registration
│           ├── actions.rs          # UI-level actions
│           │
│           ├── layout/             # Layout chính của app
│           │   ├── mod.rs
│           │   ├── workspace.rs    # DockArea tổng
│           │   ├── sidebar.rs      # Sidebar host list
│           │   └── statusbar.rs    # Status bar dưới cùng
│           │
│           ├── views/              # Các màn hình lớn
│           │   ├── mod.rs
│           │   ├── host_manager/
│           │   │   ├── mod.rs
│           │   │   ├── host_list.rs
│           │   │   ├── host_form.rs
│           │   │   └── host_card.rs
│           │   ├── terminal/
│           │   │   ├── mod.rs
│           │   │   ├── terminal_view.rs   # View trait impl
│           │   │   ├── terminal_panel.rs  # PanelView (dock)
│           │   │   └── terminal_element.rs # Custom element render
│           │   ├── sftp/
│           │   │   ├── mod.rs
│           │   │   ├── file_browser.rs
│           │   │   ├── file_list.rs
│           │   │   └── transfer_queue.rs
│           │   ├── session_tabs/
│           │   │   ├── mod.rs
│           │   │   └── tabs.rs
│           │   └── settings/
│           │       ├── mod.rs
│           │       ├── general.rs
│           │       ├── terminal.rs
│           │       ├── appearance.rs
│           │       └── about.rs
│           │
│           ├── components/         # UI component tái sử dụng
│           │   ├── mod.rs
│           │   ├── connect_dialog.rs
│           │   ├── confirm_dialog.rs
│           │   ├── empty_state.rs
│           │   └── toast.rs
│           │
│           ├── state/              # AppState chia sẻ
│           │   ├── mod.rs
│           │   ├── app_state.rs
│           │   ├── session_state.rs
│           │   └── ui_state.rs
│           │
│           └── icons.rs            # IconName constants
│
├── docs/                           # Tài liệu phát triển
│   ├── architecture.md
│   ├── ssh-flow.md
│   ├── theming.md
│   └── agents/                     # AGENTS files (tách nhỏ)
│       ├── code-style.md           # Quy ước code
│       ├── dependencies.md         # Deps + rev lock + reference-first
│       └── structure.md            # File này
│
└── reference/                      # Git submodule / clone local của gpui-component
    └── gpui-component/
```

## 2. Quy tắc cấu trúc

- **Mỗi file Rust tối đa ~400 dòng.** Nếu vượt → tách module con ngay. Ngược lại, **không tách quá nhỏ** dẫn đến 1 file chỉ có 5–10 dòng. Mỗi file phải có đủ "khối lượng trách nhiệm" để tồn tại độc lập.
- **Một module, một trách nhiệm.** Tên file = tên module chính (snake_case).
- **Folder `views/<feature>/`** cho mỗi màn hình lớn: `mod.rs` (re-export + state), `<feature>_view.rs` (Render), `<feature>_panel.rs` (nếu cần dock), `<feature>_element.rs` (nếu custom element).
- **Folder `components/`** chỉ chứa widget thuần, không phụ thuộc domain.
- **Folder `state/`** chứa `Entity<T>` state toàn cục; UI chỉ đọc/ghi qua `cx.global::<AppState>()` hoặc `cx.entity::<T>()`.
- **Không** đặt logic giao thức (ssh, local) trong crate `ui`. UI chỉ gọi qua trait abstraction (vd. `TerminalSession`).

## 3. Trách nhiệm từng crate

| Crate | Phụ thuộc | Trách nhiệm |
|---|---|---|
| `app` | `ui`, `ssh`, `local`, `core` | Binary entry point. Wire up state, mở window, kết nối event. |
| `core` | _(không — leaf crate)_ | Domain types, traits (`TerminalSession`, `FileTransfer`), `AppError`. Không phụ thuộc `gpui`. |
| `ssh` | `core` | Triển khai `russh`: client, channel, SFTP, known_hosts, auth. Implement `TerminalSession` + `FileTransfer`. |
| `local` | `core` | Triển khai PTY (`portable-pty`). Implement `TerminalSession`. |
| `ui` | `core` _(không `ssh`/`local`)_ | Toàn bộ gpui: view, layout, theme, state toàn cục. Giao tiếp với `ssh`/`local` qua trait. |

> 🔗 **Quy tắc phụ thuộc**: `app → {ui, ssh, local, core}`, `ui → core`, `ssh → core`, `local → core`. Không có cycle, không peer-to-peer giữa `ssh` và `local`.

## 4. Khi thêm crate / module mới

- Mở issue / TODO trước khi thêm crate mới ngoài 5 crate trên.
- Tên crate dùng `snake_case`.
- Mỗi crate mới phải có `Cargo.toml` riêng và là thành viên của workspace (`members = [...]`).
- Thêm crate vào bảng phụ thuộc ở section 3 ở trên.
- Cập nhật sơ đồ cây thư mục ở section 1.
