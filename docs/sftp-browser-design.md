# SFTP Browser — Thiết kế tích hợp

> Tài liệu thiết kế cho chức năng SFTP browser: duyệt file, upload/download,
> rename/delete trên máy chủ từ xa — chạy song song với terminal shell trong
> cùng 1 kết nối SSH.
>
> **Tham chiếu liên quan:**
> - [`docs/terminal-backend.md`](terminal-backend.md) §7 — thiết kế `SshSession`
> - [`docs/ssh-client-connect.md`](ssh-client-connect.md) — flow kết nối SSH
> - [`docs/gui-layout.md`](gui-layout.md) — DockArea, Panel, layout right dock
> - [`docs/agents/structure.md`](agents/structure.md) — cấu trúc crate

## Mục lục

1. [Tổng quan & mục tiêu](#1-tổng-quan--mục-tiêu)
2. [Hiện trạng project](#2-hiện-trạng-project)
3. [Kiến trúc SFTP backend](#3-kiến-trúc-sftp-backend)
4. [Tích hợp UI — SftpPanel](#4-tích-hợp-ui--sftppanel)
5. [Implementation roadmap](#5-implementation-roadmap)

---

## 1. Tổng quan & mục tiêu

### 1.1. Mô tả chức năng

Khi user mở SSH session, ứng dụng mở **một kết nối TCP SSH duy nhất** và tạo
**2 channel song song** trên kết nối đó:

| Channel | Loại | Mục đích |
|---------|------|----------|
| #1 | `session` (shell + PTY) | Terminal panel — tương tác shell |
| #2 | `subsystem@sftp` | SFTP browser panel — duyệt/thao tác file |

Cả 2 channel chia sẻ cùng 1 TCP socket, multiplex bởi SSH protocol.
Terminal và SFTP **hoàn toàn song song** — upload file không block terminal.

### 1.2. Yêu cầu chức năng

| # | Yêu cầu | Trạng thái |
|---|---------|------------|
| R1 | Mở SFTP channel cùng lúc shell, cùng 1 TCP connection | ✅ Đã xong |
| R2 | Terminal panel — shell tương tác (đã hoạt động) | ✅ Đã xong |
| R3 | SFTP browser panel — hiển thị folder tree | ✅ Đã xong |
| R4 | File operations: open/rename/delete/upload/download | ✅ Đã xong |
| R5 | File/folder detail dialog (size, perms, modified time) | ✅ Đã xong |
| R6 | Song song: upload mientras gõ lệnh terminal | ✅ Đã xong |
| R7 | SFTP optional — server không hỗ trợ thì terminal vẫn dùng được | ✅ Đã xong |

### 1.3. Sơ đồ tổng quan

```
┌─────────────────────────────────────────────────────────────┐
│                        myTerm2 app                          │
│  ┌────────────────────────┬──────────────────────────────┐  │
│  │ Terminal Panel (center)│ SFTP Browser Panel (right)   │  │
│  │  shell tương tác       │  folder tree + file ops      │  │
│  │       ↑↓               │       ↑↓                     │  │
│  │   Channel #1           │   Channel #2 (sftp)          │  │
│  └────────┬───────────────┴──────────┬───────────────────┘  │
│           │    1 SSH Connection      │                      │
│           │    (1 TCP socket)        │                      │
│           └──────────┬───────────────┘                      │
│                 russh::client::Handle                       │
└───────────────────────┬─────────────────────────────────────┘
                        │ Internet (TCP)
                 ┌──────┴──────┐
                 │  SSH server  │
                 │  sshd        │
                 └─────────────┘
```

### 1.4. Nguyên tắc thiết kế

1. **Cùng TCP connection** — 2 channel multiplex trên 1 socket, không mở connection riêng.
2. **SFTP optional** — mở SFTP channel sau shell; nếu fail → terminal vẫn hoạt động.
3. **Bridge sync↔async** — UI thread (GPUI) gọi sync, SFTP operations chạy trong tokio
   task ẩn (giống pattern `Cmd`/`SshListener` hiện tại).
4. **Không phá `TerminalSession` trait** — SFTP capability truy cập qua method riêng
   hoặc trait phụ, không ép local session implement SFTP.
5. **Tuân thủ kiến trúc crate** — SFTP backend nằm trong `crates/ssh`, UI nằm trong
   `crates/ui/src/views/sftp/`, giao tiếp qua abstraction.
---

## 2. Hiện trạng project

### 2.1. Cấu trúc workspace

```
myTerm2/
├── Cargo.toml                     # Workspace root — 5 crate members
├── crates/
│   ├── app/                       # Binary: main.rs + window.rs
│   ├── core/                      # Domain model (no GPUI) — TerminalSession trait
│   ├── ssh/                       # SSH client (russh) — ĐÃ HOẠT ĐỘNG
│   ├── local/                     # Local shell (alacritty_terminal + ConPTY)
│   └── ui/                        # GPUI + gpui-component — toàn bộ UI
└── docs/
```

**Dependency graph:**
```
app → {ui, ssh, local, core}
ui  → core          (KHÔNG import ssh/local — giao tiếp qua trait)
ssh → core
local → core
```

### 2.2. SSH crate — trạng thái hiện tại (đã hoạt động)

```
crates/ssh/src/
├── lib.rs              # Re-export: connect, SshConfig, SshSession, PtySize, Cmd
├── config.rs           # SshConfig + SshAuthMethod (Password/PrivateKey/Agent)
├── handler.rs          # SshClientHandler — check_server_key (MVP: accept all)
├── session.rs          # connect() — russh connect + auth + pty + shell → SshSession
├── session_terminal.rs # impl TerminalSession for SshSession (347 dòng)
├── listener.rs         # SshListener — EventListener + Cmd channel (bridge sync→async)
├── task.rs             # ssh_main_task — tokio task đọc channel + xử lý Cmd
└── state.rs            # SessionState — cache (title, cwd, clipboard, alive, exit_code)
```

### 2.3. SSH connection flow hiện tại

```
connect_dialog.rs → on_connect_click()
  │
  ├── Tạo SshConfig { host, port, username, auth: Password }
  │
  └── window.spawn → background_executor → ssh_connect(cfg, pty_size, scrollback)
        │
        └── SshSession::connect() [block_on trong tokio runtime]
              │
              1. russh::client::connect(addr, handler)  → Handle
              2. authenticate_password / authenticate_publickey
              3. handle.channel_open_session()           → Channel
              4. channel.request_pty("xterm-256color", cols, rows)
              5. channel.request_shell()
              6. tokio::spawn(ssh_main_task(handle, channel, ...))
                 ↑ handle bị MOVE vào task — giữ sống connection
              7. Return SshSession { term, listener, state, cmd_tx, runtime }
                 ↑ implement TerminalSession (chỉ terminal, không SFTP)
```

### 2.4. Vấn đề cốt lõi cần giải quyết

| # | Vấn đề | Chi tiết | Giải pháp |
|---|--------|----------|-----------|
| P1 | **Handle bị move vào ssh_main_task** | `russh::client::Handle` bị move vào `ssh_main_task` và giữ đến khi session đóng. Không thể mở SFTP channel sau khi connect. | Mở SFTP channel **trong `connect()` block_on** (trước khi spawn), tách SFTP channel ra độc lập. Handle vẫn move vào shell task. |
| P2 | **`TerminalSession` trait không có SFTP** | Trait hiện tại chỉ có terminal ops. Local session không có SFTP. | Thêm method `fn sftp(&self) -> Option<&SftpSession>` với default `None`, hoặc trait riêng `SftpCapable`. |
| P3 | **SftpPanel là global, SSH session là per-tab** | SftpPanel nằm ở right dock (1 panel). SSH session là tab trong center dock. | Track `active_sftp` trong `AppState`. Khi user switch tab → swap SFTP backend. |
| P4 | **SftpPanel chỉ là placeholder** | Render `"No SFTP connection."` — không có state, không có file tree. | Thay bằng file tree + toolbar + transfer queue. |
| P5 | **Chưa có `russh-sftp` dependency** | `Cargo.toml` workspace chưa khai báo. | Thêm `russh-sftp = "2.0"` vào `[workspace.dependencies]`. |

### 2.5. SftpPanel hiện tại (skeleton)

```rust
// crates/ui/src/views/sftp/file_browser.rs — HIỆN TẠI

pub struct SftpPanel {
    focus_handle: FocusHandle,
    // KHÔNG có state nào khác
}

impl Panel for SftpPanel {
    fn panel_name(&self) -> &'static str { "sftp" }
    fn title(&mut self, ...) -> impl IntoElement { "SFTP Browser" }
    fn closable(&self) -> bool { true }
    fn zoomable(&self) -> Option<PanelControl> { Some(PanelControl::Both) }
}

impl Render for SftpPanel {
    fn render(&mut self, _window, cx) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No SFTP connection.")   // ← PLACEHOLDER
    }
}
```

### 2.6. Layout hiện tại

```
┌─────────────────────────────────────────────────────────────┐
│  TitleBar  [myTerm2 ▾] [Edit] [Window] [Help]               │
├───────────────────────────────────────┬─────────────────────┤
│  CENTER (tabs)                        │  RIGHT DOCK (480px)  │
│  ┌──────────────────────────────────┐ │  ┌───────────────┐  │
│  │Tab1│Tab2│ + │                    │ │  │ SessionPanel  │  │
│  ├──────────────────────────────────┤ │  │ (SSH list)    │  │
│  │                                  │ │  ├───────────────┤  │
│  │  Terminal view (alacritty grid)  │ │  │ SftpPanel     │  │
│  │                                  │ │  │ (placeholder) │  │
│  └──────────────────────────────────┘ │  └───────────────┘  │
├───────────────────────────────────────┴─────────────────────┤
│  🕐 2025-01-15 14:32:07           [Toggle Right Dock]        │
└─────────────────────────────────────────────────────────────┘
```

- Right dock = `v_split([SessionPanel, SftpPanel])` — đặt trong `layout.rs`.
- SftpPanel đã đăng ký trong `PanelRegistry` (`ui/src/lib.rs`).
- DockPanel có thể collapse/resize — đã cấu hình sẵn.
---


## 3. Kiến trúc SFTP backend

### 3.1. Tổng quan channel multiplexing

SSH protocol hỗ trợ **nhiều channel trên 1 TCP connection**. Mỗi channel có ID
riêng, dữ liệu được tag bằng channel ID khi gửi qua socket. Đây là cơ chế native
của SSH — không cần workaround.

```
SSH TCP Connection (1 socket)
  │
  ├── Channel #1 (session) ──→ ssh_main_task  ──→ Terminal Panel
  │     request_pty → request_shell
  │     stdin/stdout stream, PTY, OSC parsing
  │     ↑ user gõ lệnh ở terminal
  │
  └── Channel #2 (session) ──→ sftp_task     ──→ SFTP Browser Panel
        request_subsystem("sftp")
        readdir, open(R/W), rename, remove, stat, mkdir
        ↑ user upload/download/duyệt file
```

**Tại sao song song được?**
- 2 channel = 2 luồng dữ liệu độc lập, multiplex trên cùng TCP socket bởi russh.
- `ssh_main_task` loop `tokio::select!` đọc channel shell.
- `sftp_task` loop riêng, xử lý `SftpCmd` từ `async_channel`.
- Tokio scheduler chia thời gian — **không block nhau**.
- Upload file 1GB: `sftp_task` stream chunks, terminal vẫn nhận keystroke ngay.

### 3.2. Thay đổi `connect()` — mở SFTP channel trước khi spawn

```
connect() flow MỚI:
  1. russh::client::connect(addr, handler)     → Handle
  2. authenticate_password / authenticate_publickey
  3. handle.channel_open_session()              → shell_channel
  4. shell_channel.request_pty(...)
  5. shell_channel.request_shell()

  ── MỚI: Thử mở SFTP channel (optional) ──
  6. match open_sftp_channel(&handle).await {
       Ok(sftp_channel) → {
           sftp_channel.request_subsystem(true, "sftp").await
           let sftp = SftpSession::new(sftp_channel.into_stream()).await
           // Spawn SFTP task
           tokio::spawn(sftp_task(sftp, sftp_cmd_rx, sftp_event_tx))
           Some(sftp_session)
       }
       Err(e) → {
           log::warn!("SFTP not available: {e}")
           None    // terminal vẫn hoạt động bình thường
       }
     }

  7. tokio::spawn(ssh_main_task(handle, shell_channel, ...))
     ↑ handle vẫn move vào shell task (giữ connection sống)
     ↑ SFTP channel đã tách ra độc lập, không cần handle nữa

  8. Return SshSession { ..., sftp: Option<SftpSession> }
```

**Key insight:** `russh::client::Handle` cho phép mở nhiều channel. Nhưng sau khi
spawn `ssh_main_task`, handle bị move. Giải pháp: **mở SFTP channel trong
`block_on` (trước spawn)**, tách SFTP channel ra object riêng. Handle chỉ cần
giữ sống (trong shell task) để connection không đóng.

### 3.3. File layout mới trong `crates/ssh/`

```
crates/ssh/src/
├── lib.rs              # Thêm: pub mod sftp; pub use sftp::{SftpSession, SftpCmd, ...}
├── config.rs           # (không đổi)
├── handler.rs          # (không đổi)
├── session.rs          # Thêm: sftp field + mở SFTP channel trong connect()
├── session_terminal.rs # Thêm: fn sftp() -> Option<&SftpSession>
├── listener.rs         # (không đổi)
├── task.rs             # (không đổi — chỉ shell channel)
├── state.rs            # (không đổi)
├── sftp.rs             # MỚI — SftpSession struct + SftpCmd + SftpEvent + FileEntry
└── sftp_task.rs        # MỚI — tokio task xử lý SFTP commands
```

### 3.4. `sftp.rs` — SftpSession + types

```rust
// crates/ssh/src/sftp.rs

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_channel::{Sender, Receiver};
use tokio::sync::oneshot;

use myterm2_core::Result;

// ── File entry cho UI rendering ──────────────────────────────

/// Một entry trong thư mục (file hoặc folder).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
}

/// File/folder metadata — cho detail dialog.
#[derive(Debug, Clone)]
pub struct FileStat {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
    pub uid: u32,
    pub gid: u32,
}

// ── Lệnh SFTP: UI → tokio task ───────────────────────────────

/// Lệnh SFTP từ UI thread gửi tới tokio task qua `async_channel`.
pub enum SftpCmd {
    /// Đọc thư mục → trả về danh sách entry.
    ReadDir {
        path: PathBuf,
        reply: oneshot::Sender<Result<Vec<FileEntry>>>,
    },
    /// Lấy metadata của 1 file/folder.
    Stat {
        path: PathBuf,
        reply: oneshot::Sender<Result<FileStat>>,
    },
    /// Đổi tên file/folder.
    Rename {
        from: PathBuf,
        to: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Xoá file.
    Remove {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Xoá thư mục rỗng.
    Rmdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Tạo thư mục.
    Mkdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Upload file local → remote. Progress qua `progress_tx` (0.0–1.0).
    Upload {
        local: PathBuf,
        remote: PathBuf,
        progress: async_channel::Sender<f64>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Download file remote → local. Progress qua `progress_tx` (0.0–1.0).
    Download {
        remote: PathBuf,
        local: PathBuf,
        progress: async_channel::Sender<f64>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Đóng SFTP session.
    Close,
}

// ── SFTP event: tokio task → UI ──────────────────────────────

/// Event từ SFTP task gửi tới UI (qua `async_channel`).
#[derive(Debug, Clone)]
pub enum SftpEvent {
    /// SFTP session đã sẵn sàng (sau khi handshake).
    Ready,
    /// SFTP session bị lỗi/ngắt.
    Error(String),
    /// SFTP session đã đóng.
    Closed,
}

// ── SftpSession — bridge sync (UI) ↔ async (tokio task) ──────

/// SFTP session — gửi lệnh qua channel, nhận event qua channel.
/// Tương tự `SshSession` pattern: UI gọi sync, tokio task xử lý async.
pub struct SftpSession {
    /// Channel gửi `SftpCmd` tới tokio task.
    cmd_tx: Sender<SftpCmd>,
    /// Channel nhận `SftpEvent` từ tokio task (UI subscribe).
    event_rx: Mutex<Option<Receiver<SftpEvent>>>,
    /// SFTP có alive không (channel chưa đóng).
    alive: Arc<Mutex<bool>>,
}

impl SftpSession {
    /// Gửi lệnh ReadDir — trả về kết quả qua oneshot (blocking).
    pub fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::ReadDir { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    /// Gửi lệnh Stat.
    pub fn stat(&self, path: PathBuf) -> Result<FileStat> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Stat { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    /// Đổi tên.
    pub fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rename { from, to, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    /// Xoá file.
    pub fn remove(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Remove { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    /// Tạo thư mục.
    pub fn mkdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Mkdir { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    /// Upload file — progress qua channel (non-blocking, fire-and-forget).
    /// Dùng `cx.spawn` để chạy async, UI observe progress channel.
    pub fn upload(
        &self,
        local: PathBuf,
        remote: PathBuf,
    ) -> (Receiver<f64>, oneshot::Receiver<Result<()>>) {
        let (progress_tx, progress_rx) = async_channel::bounded(100);
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.try_send(SftpCmd::Upload {
            local,
            remote,
            progress: progress_tx,
            reply: reply_tx,
        });
        (progress_rx, reply_rx)
    }

    /// Download file — tương tự upload.
    pub fn download(
        &self,
        remote: PathBuf,
        local: PathBuf,
    ) -> (Receiver<f64>, oneshot::Receiver<Result<()>>) {
        let (progress_tx, progress_rx) = async_channel::bounded(100);
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.try_send(SftpCmd::Download {
            remote,
            local,
            progress: progress_tx,
            reply: reply_tx,
        });
        (progress_rx, reply_rx)
    }

    /// Đóng SFTP session.
    pub fn close(&self) {
        let _ = self.cmd_tx.try_send(SftpCmd::Close);
    }

    /// Subscribe event (Ready/Error/Closed).
    pub fn subscribe(&self) -> Receiver<SftpEvent> {
        self.event_rx
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| async_channel::bounded(1).1)
    }

    /// SFTP còn sống?
    pub fn alive(&self) -> bool {
        *self.alive.lock().unwrap()
    }
}
```

### 3.5. `sftp_task.rs` — tokio task xử lý SFTP commands

```rust
// crates/ssh/src/sftp_task.rs

use std::path::PathBuf;

use russh_sftp::client::SftpSession as SftpChannel;
use async_channel::{Sender, Receiver};
use tokio::sync::oneshot;

use myterm2_core::Result;

use crate::sftp::{SftpCmd, SftpEvent, FileEntry, FileStat};

/// Tokio task xử lý SFTP commands.
/// Chạy song song với `ssh_main_task` trên cùng tokio runtime.
pub(crate) async fn sftp_task(
    sftp: SftpChannel,
    cmd_rx: Receiver<SftpCmd>,
    event_tx: Sender<SftpEvent>,
    alive: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    log::info!("sftp_task: started");
    let _ = event_tx.try_send(SftpEvent::Ready);

    loop {
        // Nhận lệnh từ UI
        match cmd_rx.recv().await {
            Ok(SftpCmd::ReadDir { path, reply }) => {
                let result = sftp_read_dir(&sftp, &path).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Stat { path, reply }) => {
                let result = sftp_stat(&sftp, &path).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rename { from, to, reply }) => {
                let result = sftp.rename(&from, &to).await
                    .map_err(|e| myterm2_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Remove { path, reply }) => {
                let result = sftp.remove_file(&path).await
                    .map_err(|e| myterm2_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rmdir { path, reply }) => {
                let result = sftp.remove_dir(&path).await
                    .map_err(|e| myterm2_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Mkdir { path, reply }) => {
                let result = sftp.create_dir(&path).await
                    .map_err(|e| myterm2_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Upload { local, remote, progress, reply }) => {
                let result = sftp_upload(&sftp, &local, &remote, &progress).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Download { remote, local, progress, reply }) => {
                let result = sftp_download(&sftp, &remote, &local, &progress).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Close) => {
                log::info!("sftp_task: close requested");
                break;
            }
            Err(_) => {
                log::info!("sftp_task: cmd_rx closed — session dropped");
                break;
            }
        }
    }

    {
        let mut a = alive.lock().unwrap();
        *a = false;
    }
    let _ = event_tx.try_send(SftpEvent::Closed);
    log::info!("sftp_task: exiting");
}

/// Đọc thư mục — chuyển `DirEntry` của russh-sftp sang `FileEntry`.
async fn sftp_read_dir(
    sftp: &SftpChannel,
    path: &PathBuf,
) -> Result<Vec<FileEntry>> {
    let mut entries = sftp.read_dir(path).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;

    let mut result = Vec::new();
    while let Some(entry) = entries.next().await {
        let entry = entry.map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let metadata = entry.metadata();
        result.push(FileEntry {
            name: name.clone(),
            path: path.join(&name),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.is_symlink(),
            size: metadata.len().unwrap_or(0),
            modified: metadata.modified().ok(),
            permissions: metadata.permissions().map(|p| p.bits()).unwrap_or(0),
            uid: metadata.uid().unwrap_or(0),
            gid: metadata.gid().unwrap_or(0),
        });
    }
    // Sort: folder trước, rồi file theo tên.
    result.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(result)
}

/// Upload file với progress reporting.
async fn sftp_upload(
    sftp: &SftpChannel,
    local: &PathBuf,
    remote: &PathBuf,
    progress: &Sender<f64>,
) -> Result<()> {
    let local_data = tokio::fs::read(local).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let total = local_data.len() as u64;

    let mut remote_file = sftp.open(remote).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;

    // Write theo chunk 32KB — gửi progress sau mỗi chunk.
    const CHUNK: usize = 32 * 1024;
    let mut written: u64 = 0;
    for chunk in local_data.chunks(CHUNK) {
        remote_file.write_all(chunk).await
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        written += chunk.len() as u64;
        let pct = if total > 0 { written as f64 / total as f64 } else { 1.0 };
        let _ = progress.try_send(pct);
    }
    remote_file.flush().await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let _ = progress.try_send(1.0);
    Ok(())
}

/// Download file với progress reporting.
async fn sftp_download(
    sftp: &SftpChannel,
    remote: &PathBuf,
    local: &PathBuf,
    progress: &Sender<f64>,
) -> Result<()> {
    let mut remote_file = sftp.open(remote).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let metadata = sftp.metadata(remote).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let total = metadata.len().unwrap_or(0);

    let mut local_file = tokio::fs::File::create(local).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;

    const CHUNK: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut read: u64 = 0;
    loop {
        let n = remote_file.read(&mut buf).await
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        if n == 0 { break; }
        local_file.write_all(&buf[..n]).await
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        read += n as u64;
        let pct = if total > 0 { read as f64 / total as f64 } else { 1.0 };
        let _ = progress.try_send(pct);
    }
    local_file.flush().await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let _ = progress.try_send(1.0);
    Ok(())
}

/// Lấy metadata chi tiết.
async fn sftp_stat(
    sftp: &SftpChannel,
    path: &PathBuf,
) -> Result<FileStat> {
    let metadata = sftp.metadata(path).await
        .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
    let name = path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(FileStat {
        name,
        path: path.clone(),
        is_dir: metadata.is_dir(),
        is_symlink: metadata.is_symlink(),
        size: metadata.len().unwrap_or(0),
        modified: metadata.modified().ok(),
        accessed: metadata.accessed().ok(),
        permissions: metadata.permissions().map(|p| p.bits()).unwrap_or(0),
        uid: metadata.uid().unwrap_or(0),
        gid: metadata.gid().unwrap_or(0),
    })
}
```

### 3.6. Thay đổi `session.rs` — thêm SFTP vào `connect()`

```rust
// crates/ssh/src/session.rs — THAY ĐỔI

pub struct SshSession {
    pub(crate) term: Arc<FairMutex<Term<SshListener>>>,
    pub(crate) listener: SshListener,
    pub(crate) event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    pub(crate) state: SharedState,
    pub(crate) cmd_tx: async_channel::Sender<Cmd>,
    pub(crate) _runtime: tokio::runtime::Runtime,
    pub(crate) cell_width: Mutex<f32>,
    pub(crate) line_height: Mutex<f32>,
    pub(crate) marked_text: Mutex<Option<String>>,
    // MỚI:
    pub(crate) sftp: Mutex<Option<SftpSession>>,
}
```

Trong `connect()` block_on, sau khi shell channel opened:

```rust
// ── MỚI: Thử mở SFTP channel (optional) ──────────────────────
let sftp_session: Option<SftpSession> = match open_sftp(&handle).await {
    Ok(sftp) => {
        log::info!("SshSession: SFTP channel opened");
        Some(sftp)
    }
    Err(e) => {
        log::warn!("SshSession: SFTP not available: {e} — terminal only");
        None
    }
};

// Spawn shell task (handle move vào — SFTP channel đã tách ra)
tokio::spawn(ssh_main_task(handle, channel, ...));

// SFTP session đã có cmd_tx/event_rx — spawn task riêng
if let Some(ref sftp) = sftp_session {
    // sftp_task đã spawn bên trong open_sftp()
}
```

```rust
/// Mở SFTP channel trên cùng handle + spawn sftp_task.
async fn open_sftp(
    handle: &russh::client::Handle<SshClientHandler>,
) -> anyhow::Result<SftpSession> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let sftp_channel = russh_sftp::client::SftpSession::new(
        channel.into_stream(),
    ).await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let (cmd_tx, cmd_rx) = async_channel::bounded::<SftpCmd>(64);
    let (event_tx, event_rx) = async_channel::bounded::<SftpEvent>(40);
    let alive = Arc::new(Mutex::new(true));

    tokio::spawn(sftp_task(
        sftp_channel,
        cmd_rx,
        event_tx,
        alive.clone(),
    ));

    Ok(SftpSession {
        cmd_tx,
        event_rx: Mutex::new(Some(event_rx)),
        alive,
    })
}
```

### 3.7. Dependency mới

```toml
# Cargo.toml [workspace.dependencies] — THÊM:
russh-sftp = "2.0"

# crates/ssh/Cargo.toml [dependencies] — THÊM:
russh-sftp.workspace = true
```
---


## 4. Tích hợp UI — SftpPanel

### 4.1. Vấn đề: SftpPanel global vs SSH session per-tab

```
Center Dock (tabs)              Right Dock
┌──────────────────────┐       ┌──────────────────┐
│ Tab1: local shell    │       │ SessionPanel     │
│ Tab2: SSH prod       │  ←→   ├──────────────────┤
│ Tab3: SSH dev        │       │ SftpPanel        │
│ Tab4: SSH staging    │       │ (1 panel cho tất │
└──────────────────────┘       │  cả tab)         │
                               └──────────────────┘
```

SftpPanel là **1 panel duy nhất** ở right dock. Nhưng có thể có nhiều SSH tab
trong center dock. Cần cơ chế:

1. Biết **tab nào đang active** trong center dock.
2. Nếu active tab là SSH session **có SFTP** → SftpPanel hiển thị file tree.
3. Nếu active tab là local shell hoặc SSH không có SFTP → SftpPanel hiển thị
   "No SFTP connection."
4. Khi user switch tab → SftpPanel swap sang SFTP backend của tab mới.

### 4.2. Giải pháp: AppState tracking active SFTP

```rust
// crates/ui/src/state/app_state.rs — THAY ĐỔI

use std::sync::Arc;

pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    // MỚI: SFTP backend của active SSH tab.
    // None = không có SFTP (local shell, hoặc SSH server không hỗ trợ SFTP).
    pub active_sftp: Option<Arc<SftpSession>>,
}
```

**Flow:**
1. `connect_dialog.rs` → `ssh_connect()` thành công → trả về `SshSession`
   (có `sftp: Option<SftpSession>`)
2. `TerminalPanel::from_session()` tạo panel — nếu session có SFTP:
   - Trích `SftpSession` ra (hoặc giữ `Arc` reference)
   - Set `AppState.active_sftp = Some(sftp)`
3. `TerminalPanel::set_active(true/false)` — hook khi tab active đổi:
   - `set_active(true)` + session có SFTP → set `active_sftp`
   - `set_active(false)` → nếu `active_sftp` đang là session này → set `None`
4. `SftpPanel` observe `AppState.active_sftp` → re-render khi thay đổi

### 4.3. Cách truy cập SFTP từ TerminalPanel

**Option A: Downcast `dyn TerminalSession` thành `SshSession`**

```rust
// Thêm method trên TerminalSession trait:
fn sftp(&self) -> Option<Arc<SftpSession>> { None }

// SshSession impl:
fn sftp(&self) -> Option<Arc<SftpSession>> {
    self.sftp.lock().unwrap().clone()
}
```

→ Đơn giản nhất. Local session trả về `None` (default). UI không cần biết
kiểu cụ thể, chỉ gọi `session.sftp()`.

**Option B: Trait riêng `SftpCapable`**

```rust
pub trait SftpCapable {
    fn sftp(&self) -> Option<&SftpSession>;
}
```

→ Phức tạp hơn, cần downcast. Không khuyến nghị.

→ **Chọn Option A** — thêm `fn sftp()` vào `TerminalSession` trait với
default `None`.

### 4.4. SftpPanel — state mới

```rust
// crates/ui/src/views/sftp/file_browser.rs — THAY ĐỔI

use std::path::PathBuf;
use gpui_component::list::ListItem;

pub struct SftpPanel {
    focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    /// SFTP session hiện đang hiển thị (từ active SSH tab).
    /// None = không có SFTP connection.
    sftp: Option<Arc<SftpSession>>,

    // ── File tree state ─────────────────────────────────────
    /// Thư mục hiện tại đang hiển thị.
    cwd: PathBuf,
    /// Danh sách entry trong `cwd` (cache).
    entries: Vec<FileEntry>,
    /// Entry đang được select (click 1 lần).
    selected: Option<usize>,
    /// Đang loading (đợi ReadDir response).
    loading: bool,
    /// Lỗi gần nhất (hiển thị inline).
    error: Option<String>,

    // ── Transfer queue ──────────────────────────────────────
    /// Danh sách transfer đang chạy/chờ.
    transfers: Vec<TransferItem>,

    // ── Sort state ──────────────────────────────────────────
    sort_by: SortBy,
    sort_asc: bool,
}

/// 1 item trong transfer queue.
pub struct TransferItem {
    pub direction: TransferDirection,  // Upload / Download
    pub filename: String,
    pub local: PathBuf,
    pub remote: PathBuf,
    pub progress: f64,      // 0.0 – 1.0
    pub status: TransferStatus,  // Pending / InProgress / Done / Error
    pub error: Option<String>,
}

pub enum TransferDirection { Upload, Download }
pub enum TransferStatus { Pending, InProgress, Done, Error }

pub enum SortBy { Name, Size, Modified, Type }
```

### 4.5. SftpPanel — Render

```
┌─ SFTP Browser ──────────────────────────────────────────┐
│  /home/user/projects/                    [↑] [⟳] [🔍]   │  ← breadcrumb + toolbar
│                                                         │
│  📁 ../                                                  │  ← parent dir
│  ▸ 📁 src/                                               │  ← folder (expandable)
│  ▸ 📁 tests/                                             │
│  📄 Cargo.toml      1.2 KB   Jan 14 02:00               │  ← file
│  📄 README.md       4.5 KB   Jan 13 18:45               │
│  📄 deploy.sh       1.8 KB   Jan 12 14:30  [SELECTED]   │
│                                                         │
│  ─────────────────────────────────────────────────────  │
│  [⬆ Upload]  [⬇ Download]  [✏ Rename]  [🗑 Delete]      │  ← action toolbar
│  [📁 New Folder]  [📋 Properties]                       │
│  ─────────────────────────────────────────────────────  │
│  Transfers:                                             │  ← transfer queue
│  ⬆ bigfile.zip   ▓▓▓▓▓░░░░░ 56%  2.3MB/s               │
│  ⬇ config.toml   ▓▓▓▓▓▓▓▓▓▓ 100% ✓                      │
│  ⬆ data.csv      ▓░░░░░░░░░ 12%  waiting...             │
└─────────────────────────────────────────────────────────┘
```

**Render flow:**
```rust
impl Render for SftpPanel {
    fn render(&mut self, window, cx) -> impl IntoElement {
        let sftp = self.sftp.clone();

        match &sftp {
            None => self.render_no_connection(cx),
            Some(_) => div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_breadcrumb(cx))      // path + toolbar
                .child(self.render_file_list(cx))        // entries
                .child(self.render_action_toolbar(cx))   // buttons
                .child(self.render_transfer_queue(cx)),  // transfers
        }
    }
}
```

### 4.6. File operations — UI flow

| Operation | Trigger | Flow |
|-----------|---------|------|
| **Navigate** | Double-click folder / click `↑` | `sftp.read_dir(path)` → update `entries` |
| **Select** | Single-click entry | Set `selected` index |
| **Rename** | Click `✏` / F2 / context menu | Dialog nhập tên mới → `sftp.rename(from, to)` |
| **Delete** | Click `🗑` / Delete key | Confirm dialog → `sftp.remove(path)` |
| **Upload** | Click `⬆` / drag-drop file | File picker → `sftp.upload(local, remote)` → add to transfer queue |
| **Download** | Click `⬇` / context menu | File picker → `sftp.download(remote, local)` → add to transfer queue |
| **New Folder** | Click `📁` | Dialog nhập tên → `sftp.mkdir(path)` |
| **Properties** | Click `📋` / context menu | `sftp.stat(path)` → detail dialog |
| **Open** | Double-click file | Download về temp → mở app local → (edit → upload lại?) |
| **Refresh** | Click `⟳` / F5 | `sftp.read_dir(cwd)` → update `entries` |

### 4.7. Transfer queue — song song với terminal

```rust
// Khi user click Upload:
fn on_upload(&mut self, local: PathBuf, cx: &mut Context<Self>) {
    let sftp = self.sftp.clone().unwrap();
    let remote = self.cwd.join(local.file_name().unwrap());

    // Thêm vào transfer queue (UI hiển thị ngay)
    let transfer_idx = self.transfers.len();
    self.transfers.push(TransferItem {
        direction: TransferDirection::Upload,
        filename: local.file_name().unwrap().to_string_lossy().into(),
        local: local.clone(),
        remote: remote.clone(),
        progress: 0.0,
        status: TransferStatus::InProgress,
        error: None,
    });
    cx.notify();

    // Start upload (async — không block UI)
    let (progress_rx, reply_rx) = sftp.upload(local, remote);

    // Spawn task để poll progress + update UI
    cx.spawn(async move |this, cx| {
        // Poll progress
        while let Ok(pct) = progress_rx.recv().await {
            _ = this.update(cx, |this, cx| {
                if let Some(t) = this.transfers.get_mut(transfer_idx) {
                    t.progress = pct;
                }
                cx.notify();
            });
        }
        // Wait for completion
        match reply_rx.await {
            Ok(Ok(())) => {
                _ = this.update(cx, |this, cx| {
                    if let Some(t) = this.transfers.get_mut(transfer_idx) {
                        t.status = TransferStatus::Done;
                    }
                    cx.notify();
                });
            }
            Ok(Err(e)) => { /* set error */ }
            Err(_) => { /* channel closed */ }
        }
    }).detach();

    // ← Trong lúc này, terminal vẫn gõ lệnh bình thường!
    //    Vì sftp_task và ssh_main_task là 2 tokio task độc lập.
}
```

### 4.8. Context menu (right-click)

```
Right-click vào file:
┌──────────────────┐
│ ⬇ Download       │
│ ✏ Rename         │
│ 🗑 Delete         │
│ ─────────────── │
│ 📋 Properties    │
└──────────────────┘

Right-click vào folder:
┌──────────────────┐
│ 📂 Open           │
│ ⬇ Download       │
│ ✏ Rename         │
│ 🗑 Delete         │
│ ─────────────── │
│ 📋 Properties    │
└──────────────────┘

Right-click vào vùng trống:
┌──────────────────┐
│ ⬆ Upload         │
│ 📁 New Folder    │
│ ⟳ Refresh        │
└──────────────────┘
```

### 4.9. File detail dialog

```
┌─ Properties ──────────────────────────┐
│                                       │
│  📄 deploy.sh                         │
│                                       │
│  Path:      /home/user/deploy.sh      │
│  Type:      File                      │
│  Size:      1,843 bytes (1.8 KB)      │
│  Modified:  2025-01-12 14:30:45       │
│  Accessed:  2025-01-14 09:15:22       │
│  Permissions: rwxr-xr-x (755)         │
│  Owner:     user (uid=1000)           │
│  Group:     user (gid=1000)           │
│                                       │
│              [Close]                  │
└───────────────────────────────────────┘
```

### 4.10. Error handling

| Tình huống | UI behavior |
|------------|-------------|
| SFTP channel fail khi connect | Terminal vẫn hoạt động. SftpPanel: "This server doesn't support SFTP." |
| ReadDir fail (permission denied) | Hiển thị error inline: "Permission denied: /root/" |
| Delete fail | Error dialog: "Cannot delete: Permission denied" |
| Upload fail (disk full remote) | Transfer queue: status = Error, message = "No space left on device" |
| Connection drop | `SftpEvent::Closed` → SftpPanel: "Connection lost." + terminal cũng disconnect |

### 4.11. Thay đổi `TerminalPanel::set_active` — swap SFTP

```rust
// crates/ui/src/views/terminal/panel.rs — THAY ĐỔI

impl Panel for TerminalPanel {
    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }

        // MỚI: Khi tab active đổi → cập nhật AppState.active_sftp
        if active {
            // Tab này thành active → trích SFTP từ session (nếu có)
            let sftp = self.view.read(cx).session.read(cx).sftp();
            AppState::global(cx).update(cx, |state, cx| {
                state.active_sftp = sftp;
                cx.notify();
            });
        }
        // Không cần set None khi deactivate — tab mới active sẽ overwrite.
    }
}
```

### 4.12. SftpPanel observe AppState

```rust
// crates/ui/src/views/sftp/file_browser.rs

impl SftpPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Observe AppState — khi active_sftp đổi, update + re-render
        let app_state = AppState::global(cx);
        cx.observe(&app_state, |this, state, cx| {
            let new_sftp = state.active_sftp.clone();
            if this.sftp.as_ref().map(|s| s.as_ref()) != new_sftp.as_ref().map(|s| s.as_ref()) {
                this.sftp = new_sftp;
                // Reset state khi đổi session
                this.entries.clear();
                this.selected = None;
                this.error = None;
                // Load root dir của session mới
                if this.sftp.is_some() {
                    this.load_dir(PathBuf::from("/"), cx);
                }
            }
            cx.notify();
        }).detach();

        Self {
            focus_handle,
            sftp: None,
            cwd: PathBuf::new(),
            entries: Vec::new(),
            selected: None,
            loading: false,
            error: None,
            transfers: Vec::new(),
            sort_by: SortBy::Name,
            sort_asc: true,
        }
    }
}
```

### 4.13. Layout file thay đổi

```
crates/ui/src/views/sftp/
├── mod.rs              # Re-export SftpPanel + submodules
├── file_browser.rs     # SftpPanel (Panel + Render) — state + main render
├── file_list.rs        # MỚI — render file list (entries, sort, select)
├── toolbar.rs          # MỚI — render breadcrumb + action toolbar
├── transfer_queue.rs   # MỚI — render transfer queue (progress bars)
├── context_menu.rs     # MỚI — context menu cho file/folder/empty area
└── detail_dialog.rs    # MỚI — file/folder properties dialog
```
---


## 5. Implementation roadmap

### 5.1. Dependency graph giữa các bước

```
Bước 1: Dependency + types
  │
  ├──→ Bước 2: SFTP backend (sftp.rs + sftp_task.rs)
  │      │
  │      └──→ Bước 3: Modify connect() — mở SFTP channel
  │             │
  │             ├──→ Bước 4: TerminalSession trait — thêm fn sftp()
  │             │
  │             └──→ Bước 5: AppState — track active_sftp
  │                    │
  │                    ├──→ Bước 6: SftpPanel — file tree + navigation
  │                    │
  │                    ├──→ Bước 7: SftpPanel — file operations
  │                    │
  │                    ├──→ Bước 8: SftpPanel — transfer queue
  │                    │
  │                    └──→ Bước 9: SftpPanel — detail dialog + context menu
```

### 5.2. Checklist chi tiết

#### Bước 1: Dependency + core types

- [x] Thêm `russh-sftp = "2.0"` vào `Cargo.toml` `[workspace.dependencies]` *(dùng 2.3)*
- [x] Thêm `russh-sftp.workspace = true` vào `crates/ssh/Cargo.toml`
- [x] Tạo `crates/ssh/src/sftp.rs` — `SftpCmd`, `SftpEvent`, `FileEntry`, `FileStat`
- [x] Thêm `pub mod sftp;` + re-export trong `crates/ssh/src/lib.rs`
- [x] `cargo build` pass

#### Bước 2: SFTP backend — sftp.rs + sftp_task.rs

- [x] Implement `SftpSession` struct (cmd_tx, event_rx, alive)
- [x] Implement sync methods: `read_dir`, `stat`, `rename`, `remove`, `mkdir`
- [x] Implement async methods: `upload`, `download` (trả về progress + reply channels)
- [x] Tạo `crates/ssh/src/sftp_task.rs` — `sftp_task()` tokio task
- [x] Implement `sftp_read_dir`, `sftp_stat`, `sftp_upload`, `sftp_download` helpers
- [x] Thêm `pub mod sftp_task;` trong `lib.rs`
- [x] `cargo build` pass

#### Bước 3: Modify connect() — mở SFTP channel

- [x] Thêm `sftp: Mutex<Option<SftpSession>>` field vào `SshSession` struct
- [x] Viết `open_sftp()` async helper — mở channel + request subsystem + spawn task
- [x] Trong `connect()` block_on: gọi `open_sftp(&handle)` sau khi shell channel opened
- [x] Xử lý `Err` → `sftp = None` (terminal vẫn hoạt động)
- [x] Spawn `sftp_task` (bên trong `open_sftp`)
- [x] Set `sftp` field trong `SshSession` return value
- [x] `cargo build` pass
- [x] `cargo run` — connect SSH, kiểm tra log "SFTP channel opened"

#### Bước 4: TerminalSession trait — thêm fn sftp()

- [x] Thêm method `fn sftp(&self) -> Option<Arc<dyn SftpBackend>>` vào `TerminalSession`
  - Default implementation: `None`
- [x] Implement cho `SshSession`: trả về `self.sftp.lock().unwrap().clone()`
  - Đã đổi `Mutex<Option<SftpSession>>` → `Mutex<Option<Arc<SftpSession>>>`
- [x] `LocalSession` — không cần impl (dùng default `None`)
- [x] Import `SftpBackend` vào `core` crate — định nghĩa trait trong `crates/core/src/sftp.rs`
  - **Giải pháp**: Định nghĩa `SftpBackend` trait trong `core`, impl cho `SftpSession` trong `ssh` crate.
- [x] `cargo build` pass + `cargo test` pass

#### Bước 5: AppState — track active_sftp

- [x] Thêm `active_sftp: Option<Arc<dyn SftpBackend>>` vào `AppState`
- [x] Trong `TerminalPanel::set_active(true)`:
  - Gọi `session.sftp()` → set `AppState.active_sftp`
- [x] `cargo build` pass

#### Bước 6: SftpPanel — file tree + navigation

- [x] Thay `SftpPanel` struct: thêm state fields (sftp, cwd, entries, selected, loading, error)
- [x] Observe `AppState` — khi `active_sftp` đổi → update + load root dir
- [x] Implement `load_dir(path)` — gọi `sftp.read_dir()` trong `cx.spawn`
- [x] Render breadcrumb (path + `↑` parent + `⟳` refresh)
- [x] Render file list (folders trước, files sau, icon + name + size + date)
- [x] Double-click folder → navigate
- [x] Single-click → select
- [x] Render "No SFTP connection." khi `sftp = None`
- [x] Render "Loading..." khi `loading = true`
- [x] Render error inline khi `error = Some(...)`
- [x] `cargo build` pass + manual test

#### Bước 7: SftpPanel — file operations

- [x] Toolbar buttons: Upload, Download, Rename, Delete, New Folder, Properties
- [x] Rename: dialog (InputState) → `sftp.rename()`
- [x] Delete: confirm dialog → `sftp.remove()` / `sftp.rmdir()`
- [x] New Folder: dialog → `sftp.mkdir()`
- [x] Upload: file picker dialog → `sftp.upload()` → add to transfer queue
- [x] Download: file picker dialog → `sftp.download()` → add to transfer queue
- [x] Refresh file list sau mỗi operation thành công
- [x] Error handling: hiển thị dialog/toast khi operation fail
- [x] `cargo build` pass + manual test

#### Bước 8: SftpPanel — transfer queue

- [x] `TransferItem` struct (direction, filename, progress, status, error)
- [x] Render transfer queue section (dưới action toolbar)
- [x] Progress bar cho mỗi transfer (0% – 100%)
- [x] Status icon: ⬆/⬇ + ✓/✗/⏳
- [x] Spawn task poll progress channel → update UI
- [x] Clear completed transfers (button hoặc auto-remove sau 5s)
- [x] Cancel transfer — `tokio_util::CancellationToken` + spawn upload/download thành task riêng, Cancel button (icon Close) trong transfer queue
- [x] `cargo build` pass + manual test (upload file lớn + gõ lệnh terminal)

#### Bước 9: SftpPanel — detail dialog + context menu

- [x] Context menu cho file: Download, Rename, Delete, Properties
- [x] Context menu cho folder: Open, Rename, Delete, Properties (không Download — folder download bị block từ bước 7)
- [x] Context menu cho empty area: Upload, New Folder, Refresh
- [x] Detail dialog: `sftp.stat()` → hiển thị size, modified, permissions, uid, gid, accessed, owner, group, path
- [x] Permissions hiển thị dạng `rwxr-xr-x (0775)`
- [x] `cargo build` pass + manual test

### 5.3. Trait abstraction cho SFTP (giải quyết core ↔ ssh)

**Vấn đề:** `core` crate là leaf (không phụ thuộc `ssh`). Không thể import
`SftpSession` từ `ssh` vào `core`.

**Giải pháp:** Định nghĩa trait abstract trong `core`:

```rust
// crates/core/src/sftp.rs (MỚI)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Abstract SFTP backend — implement bởi `ssh` crate.
/// UI dùng qua `dyn SftpBackend`, không biết `russh-sftp`.
pub trait SftpBackend: Send + Sync + 'static {
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>>;
    fn stat(&self, path: PathBuf) -> Result<FileStat>;
    fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()>;
    fn remove(&self, path: PathBuf) -> Result<()>;
    fn mkdir(&self, path: PathBuf) -> Result<()>;
    fn upload(&self, local: PathBuf, remote: PathBuf)
        -> (async_channel::Receiver<f64>, oneshot::Receiver<Result<()>>);
    fn download(&self, remote: PathBuf, local: PathBuf)
        -> (async_channel::Receiver<f64>, oneshot::Receiver<Result<()>>);
    fn close(&self);
    fn alive(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct FileEntry { /* ... */ }

#[derive(Debug, Clone)]
pub struct FileStat { /* ... */ }
```

```rust
// crates/ssh/src/sftp.rs — impl SftpBackend cho SftpSession
impl myterm2_core::SftpBackend for SftpSession {
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> { ... }
    // ...
}

// crates/core/src/terminal/session.rs — thêm vào TerminalSession trait:
fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }
```

### 5.4. File layout cuối cùng

```
crates/
├── core/src/
│   ├── sftp.rs                 # MỚI — SftpBackend trait + FileEntry + FileStat
│   ├── terminal/session.rs     # Thêm: fn sftp() -> Option<Arc<dyn SftpBackend>>
│   └── lib.rs                  # Thêm: pub mod sftp; pub use sftp::{SftpBackend, FileEntry, FileStat}
│
├── ssh/src/
│   ├── sftp.rs                 # MỚI — SftpSession + impl SftpBackend
│   ├── sftp_task.rs            # MỚI — tokio task
│   ├── session.rs              # Thêm: sftp field + open_sftp()
│   ├── session_terminal.rs     # Thêm: fn sftp() impl
│   └── lib.rs                  # Thêm: pub mod sftp; pub mod sftp_task
│
└── ui/src/
    ├── state/app_state.rs      # Thêm: active_sftp field
    ├── views/terminal/panel.rs # Thêm: set_active → update active_sftp
    └── views/sftp/
        ├── mod.rs              # Re-export + submodules
        ├── file_browser.rs     # SftpPanel — state + main render
        ├── file_list.rs        # MỚI — file list rendering
        ├── toolbar.rs          # MỚI — breadcrumb + action toolbar
        ├── transfer_queue.rs   # MỚI — transfer queue rendering
        ├── context_menu.rs     # MỚI — context menu
        └── detail_dialog.rs    # MỚI — properties dialog
```

### 5.5. Thứ tự ưu tiên

| Phase | Bước | Mục tiêu | Ước tính |
|-------|------|----------|----------|
| **Phase 1: Backend** | 1–3 | SFTP channel hoạt động, `read_dir` trả về data | |
| **Phase 2: Wiring** | 4–5 | UI truy cập được SFTP backend qua trait | |
| **Phase 3: MVP UI** | 6 | File tree hiển thị, navigate được | |
| **Phase 4: Operations** | 7 | Rename/delete/upload/download hoạt động | |
| **Phase 5: Polish** | 8–9 | Transfer queue + detail dialog + context menu | |

### 5.6. Rủi ro & giải pháp

| Rủi ro | Giải pháp |
|--------|-----------|
| `russh-sftp` API không khớp thiết kế | Đọc source `russh-sftp` trên crates.io/docs.rs trước khi implement |
| `Handle` không cho phép mở channel sau khi spawn | Mở SFTP channel trong `block_on` (trước spawn) — đã thiết kế |
| `core` crate không thể import `ssh` types | Dùng trait `SftpBackend` trong `core`, impl trong `ssh` |
| Upload/download file lớn block UI | Stream chunk + progress channel + `cx.spawn` (không block) |
| Nhiều SSH tab — SftpPanel hiển thị sai session | `AppState.active_sftp` + `set_active` hook |
| Connection drop giữa transfer | `SftpEvent::Closed` → mark transfer as Error |
| Drag-drop file vào panel | GPUI `on_drop` handler — cần check API trong reference |

### 5.7. Tham chiếu

| Cần | Đọc |
|-----|-----|
| russh channel API | `russh` docs.rs — `client::Handle`, `Channel`, `request_subsystem` |
| russh-sftp client API | `russh-sftp` docs.rs — `SftpSession`, `read_dir`, `metadata`, `open` |
| gpui-component Tree | `reference/gpui-component/crates/ui/src/tree/` |
| gpui-component List | `reference/gpui-component/crates/ui/src/list/` |
| gpui-component Dialog | `reference/gpui-component/crates/ui/src/dialog/` |
| gpui-component ContextMenu | `reference/gpui-component/crates/ui/src/menu/` |
| gpui-component Button | `reference/gpui-component/crates/ui/src/button/` |
| gpui-component Input | `reference/gpui-component/crates/ui/src/input/` |
| Pattern Cmd/SshListener | `crates/ssh/src/listener.rs` (hiện tại) |
| Pattern ssh_main_task | `crates/ssh/src/task.rs` (hiện tại) |
| SshSession::connect | `crates/ssh/src/session.rs` (hiện tại) |
| connect_dialog flow | `crates/ui/src/views/session_tabs/connect_dialog.rs` (hiện tại) |
| TerminalPanel set_active | `crates/ui/src/views/terminal/panel.rs` (hiện tại) |
---

