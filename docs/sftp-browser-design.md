# SFTP Browser — Integration design

> Design document for the SFTP browser feature: browse files, upload/download,
> rename/delete on a remote host — running in parallel with the terminal shell over
> the same SSH connection.
>
> **Related references:**
> - [`docs/terminal-backend.md`](terminal-backend.md) §7 — `SshSession` design
> - [`docs/ssh-client-connect.md`](ssh-client-connect.md) — SSH connection flow
> - [`docs/gui-layout.md`](gui-layout.md) — DockArea, Panel, right-dock layout
> - [`docs/agents/structure.md`](agents/structure.md) — crate structure

## Table of contents

1. [Overview & goals](#1-overview--goals)
2. [Project current state](#2-project-current-state)
3. [SFTP backend architecture](#3-sftp-backend-architecture)
4. [UI integration — SftpPanel](#4-ui-integration--sftppanel)
5. [Implementation roadmap](#5-implementation-roadmap)

---

## 1. Overview & goals

### 1.1. Feature description

When the user opens an SSH session, the app opens **a single SSH TCP connection** and creates
**2 parallel channels** on that connection:

| Channel | Type | Purpose |
|---------|------|----------|
| #1 | `session` (shell + PTY) | Terminal panel — shell interaction |
| #2 | `subsystem@sftp` | SFTP browser panel — browse/manipulate files |

Both channels share the same TCP socket, multiplexed by the SSH protocol.
Terminal and SFTP are **fully parallel** — uploading a file doesn't block the terminal.

### 1.2. Functional requirements

| # | Requirement | Status |
|---|---------|------------|
| R1 | Open an SFTP channel at the same time as the shell, on the same TCP connection | ✅ Done |
| R2 | Terminal panel — interactive shell (working) | ✅ Done |
| R3 | SFTP browser panel — display folder tree | ✅ Done |
| R4 | File operations: open/rename/delete/upload/download | ✅ Done |
| R5 | File/folder detail dialog (size, perms, modified time) | ✅ Done |
| R6 | Parallel: upload while typing terminal commands | ✅ Done |
| R7 | SFTP optional — if the server doesn't support it, the terminal still works | ✅ Done |

### 1.3. Overview diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        OneTerm app                          │
│  ┌────────────────────────┬──────────────────────────────┐  │
│  │ Terminal Panel (center)│ SFTP Browser Panel (right)   │  │
│  │  interactive shell     │  folder tree + file ops      │  │
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

### 1.4. Design principles

1. **Same TCP connection** — 2 channels multiplexed on 1 socket, no separate connection.
2. **SFTP optional** — open the SFTP channel after the shell; if it fails → the terminal still works.
3. **Sync↔async bridge** — the UI thread (GPUI) calls sync, SFTP operations run in a hidden
   tokio task (like the current `Cmd`/`SshListener` pattern).
4. **Don't break the `TerminalSession` trait** — SFTP capability accessed via a separate method
   or sub-trait, don't force the local session to implement SFTP.
5. **Follow the crate architecture** — the SFTP backend lives in `crates/ssh`, the UI lives in
   `crates/ui/src/views/sftp/`, communicating via an abstraction.
---

## 2. Project current state

### 2.1. Workspace structure

```
OneTerm/
├── Cargo.toml                     # Workspace root — 5 crate members
├── crates/
│   ├── app/                       # Binary: main.rs + window.rs
│   ├── core/                      # Domain model (no GPUI) — TerminalSession trait
│   ├── ssh/                       # SSH client (russh) — WORKING
│   ├── local/                     # Local shell (alacritty_terminal + ConPTY)
│   └── ui/                        # GPUI + gpui-component — all UI
└── docs/
```

**Dependency graph:**
```
app → {ui, ssh, local, core}
ui  → core          (does NOT import ssh/local — communicates via traits)
ssh → core
local → core
```

### 2.2. SSH crate — current state (working)

```
crates/ssh/src/
├── lib.rs              # Re-export: connect, SshConfig, SshSession, PtySize, Cmd
├── config.rs           # SshConfig + SshAuthMethod (Password/PrivateKey/Agent)
├── handler.rs          # SshClientHandler — check_server_key (MVP: accept all)
├── session.rs          # connect() — russh connect + auth + pty + shell → SshSession
├── session_terminal.rs # impl TerminalSession for SshSession (347 lines)
├── listener.rs         # SshListener — EventListener + Cmd channel (sync→async bridge)
├── task.rs             # ssh_main_task — tokio task reading channel + handling Cmd
└── state.rs            # SessionState — cache (title, cwd, clipboard, alive, exit_code)
```

### 2.3. Current SSH connection flow

```
connect_dialog.rs → on_connect_click()
  │
  ├── Create SshConfig { host, port, username, auth: Password }
  │
  └── window.spawn → background_executor → ssh_connect(cfg, pty_size, scrollback)
        │
        └── SshSession::connect() [block_on in tokio runtime]
              │
              1. russh::client::connect(addr, handler)  → Handle
              2. authenticate_password / authenticate_publickey
              3. handle.channel_open_session()           → Channel
              4. channel.request_pty("xterm-256color", cols, rows)
              5. channel.request_shell()
              6. tokio::spawn(ssh_main_task(handle, channel, ...))
                 ↑ handle is MOVED into the task — keeps the connection alive
              7. Return SshSession { term, listener, state, cmd_tx, runtime }
                 ↑ implements TerminalSession (terminal only, no SFTP)
```

### 2.4. Core problems to solve

| # | Problem | Detail | Solution |
|---|--------|----------|-----------|
| P1 | **Handle is moved into ssh_main_task** | `russh::client::Handle` is moved into `ssh_main_task` and held until the session closes. Can't open an SFTP channel after connect. | Open the SFTP channel **inside `connect()` block_on** (before spawn), split the SFTP channel out as independent. The handle still moves into the shell task. |
| P2 | **The `TerminalSession` trait has no SFTP** | The current trait only has terminal ops. The local session has no SFTP. | Add a method `fn sftp(&self) -> Option<&SftpSession>` with default `None`, or a separate `SftpCapable` trait. |
| P3 | **SftpPanel is global, the SSH session is per-tab** | SftpPanel is in the right dock (1 panel). The SSH session is a tab in the center dock. | Track `active_sftp` in `AppState`. When the user switches tabs → swap the SFTP backend. |
| P4 | **SftpPanel is just a placeholder** | Renders `"No SFTP connection."` — no state, no file tree. | Replace with a file tree + toolbar + transfer queue. |
| P5 | **No `russh-sftp` dependency yet** | The workspace `Cargo.toml` doesn't declare it. | Add `russh-sftp = "2.0"` to `[workspace.dependencies]`. |

### 2.5. Current SftpPanel (skeleton)

```rust
// crates/ui/src/views/sftp/file_browser.rs — CURRENT

pub struct SftpPanel {
    focus_handle: FocusHandle,
    // NO other state
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

### 2.6. Current layout

```
┌─────────────────────────────────────────────────────────────┐
│  TitleBar  [OneTerm ▾] [Edit] [Window] [Help]               │
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

- Right dock = `v_split([SessionPanel, SftpPanel])` — set in `layout.rs`.
- SftpPanel is registered in `PanelRegistry` (`ui/src/lib.rs`).
- The DockPanel can collapse/resize — already configured.
---


## 3. SFTP backend architecture

### 3.1. Channel multiplexing overview

The SSH protocol supports **multiple channels on 1 TCP connection**. Each channel has its
own ID; data is tagged with the channel ID when sent over the socket. This is SSH's native
mechanism — no workaround needed.

```
SSH TCP Connection (1 socket)
  │
  ├── Channel #1 (session) ──→ ssh_main_task  ──→ Terminal Panel
  │     request_pty → request_shell
  │     stdin/stdout stream, PTY, OSC parsing
  │     ↑ user types commands in the terminal
  │
  └── Channel #2 (session) ──→ sftp_task     ──→ SFTP Browser Panel
        request_subsystem("sftp")
        readdir, open(R/W), rename, remove, stat, mkdir
        ↑ user uploads/downloads/browses files
```

**Why is it parallel?**
- 2 channels = 2 independent data streams, multiplexed on the same TCP socket by russh.
- `ssh_main_task` loops `tokio::select!` reading the shell channel.
- `sftp_task` has its own loop, processing `SftpCmd` from `async_channel`.
- The tokio scheduler time-shares — **they don't block each other**.
- Upload a 1GB file: `sftp_task` streams chunks, the terminal still receives keystrokes instantly.

### 3.2. Changing `connect()` — open the SFTP channel before spawn

```
NEW connect() flow:
  1. russh::client::connect(addr, handler)     → Handle
  2. authenticate_password / authenticate_publickey
  3. handle.channel_open_session()              → shell_channel
  4. shell_channel.request_pty(...)
  5. shell_channel.request_shell()

  ── NEW: try to open an SFTP channel (optional) ──
  6. match open_sftp_channel(&handle).await {
       Ok(sftp_channel) → {
           sftp_channel.request_subsystem(true, "sftp").await
           let sftp = SftpSession::new(sftp_channel.into_stream()).await
           // Spawn the SFTP task
           tokio::spawn(sftp_task(sftp, sftp_cmd_rx, sftp_event_tx))
           Some(sftp_session)
       }
       Err(e) → {
           log::warn!("SFTP not available: {e}")
           None    // terminal still works normally
       }
     }

  7. tokio::spawn(ssh_main_task(handle, shell_channel, ...))
     ↑ handle still moves into the shell task (keeps the connection alive)
     ↑ the SFTP channel has been split out as independent, no longer needs the handle

  8. Return SshSession { ..., sftp: Option<SftpSession> }
```

**Key insight:** `russh::client::Handle` allows opening multiple channels. But after
spawning `ssh_main_task`, the handle is moved. Solution: **open the SFTP channel inside
`block_on` (before spawn)**, split the SFTP channel out into a separate object. The handle
only needs to stay alive (inside the shell task) so the connection doesn't close.

### 3.3. New file layout in `crates/ssh/`

```
crates/ssh/src/
├── lib.rs              # Add: pub mod sftp; pub use sftp::{SftpSession, SftpCmd, ...}
├── config.rs           # (unchanged)
├── handler.rs          # (unchanged)
├── session.rs          # Add: sftp field + open SFTP channel in connect()
├── session_terminal.rs # Add: fn sftp() -> Option<&SftpSession>
├── listener.rs         # (unchanged)
├── task.rs             # (unchanged — shell channel only)
├── state.rs            # (unchanged)
├── sftp.rs             # NEW — SftpSession struct + SftpCmd + SftpEvent + FileEntry
└── sftp_task.rs        # NEW — tokio task handling SFTP commands
```

### 3.4. `sftp.rs` — SftpSession + types

```rust
// crates/ssh/src/sftp.rs

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_channel::{Sender, Receiver};
use tokio::sync::oneshot;

use oneterm_core::Result;

// ── File entry for UI rendering ──────────────────────────────

/// An entry in a directory (file or folder).
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

/// File/folder metadata — for the detail dialog.
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

// ── SFTP command: UI → tokio task ───────────────────────────────

/// SFTP command from the UI thread sent to the tokio task via `async_channel`.
pub enum SftpCmd {
    /// Read a directory → returns a list of entries.
    ReadDir {
        path: PathBuf,
        reply: oneshot::Sender<Result<Vec<FileEntry>>>,
    },
    /// Get the metadata of a single file/folder.
    Stat {
        path: PathBuf,
        reply: oneshot::Sender<Result<FileStat>>,
    },
    /// Rename a file/folder.
    Rename {
        from: PathBuf,
        to: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Delete a file.
    Remove {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Delete an empty directory.
    Rmdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Create a directory.
    Mkdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Upload a local file → remote. Progress via `progress_tx` (0.0–1.0).
    Upload {
        local: PathBuf,
        remote: PathBuf,
        progress: async_channel::Sender<f64>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Download a remote file → local. Progress via `progress_tx` (0.0–1.0).
    Download {
        remote: PathBuf,
        local: PathBuf,
        progress: async_channel::Sender<f64>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Close the SFTP session.
    Close,
}

// ── SFTP event: tokio task → UI ──────────────────────────────

/// Event from the SFTP task sent to the UI (via `async_channel`).
#[derive(Debug, Clone)]
pub enum SftpEvent {
    /// SFTP session is ready (after handshake).
    Ready,
    /// SFTP session errored/disconnected.
    Error(String),
    /// SFTP session closed.
    Closed,
}

// ── SftpSession — bridge sync (UI) ↔ async (tokio task) ──────

/// SFTP session — sends commands via a channel, receives events via a channel.
/// Similar to the `SshSession` pattern: UI calls sync, the tokio task handles async.
pub struct SftpSession {
    /// Channel to send `SftpCmd` to the tokio task.
    cmd_tx: Sender<SftpCmd>,
    /// Channel to receive `SftpEvent` from the tokio task (UI subscribes).
    event_rx: Mutex<Option<Receiver<SftpEvent>>>,
    /// Is SFTP alive (channel not closed yet).
    alive: Arc<Mutex<bool>>,
}

impl SftpSession {
    /// Send a ReadDir command — returns the result via oneshot (blocking).
    pub fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::ReadDir { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    /// Send a Stat command.
    pub fn stat(&self, path: PathBuf) -> Result<FileStat> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Stat { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    /// Rename.
    pub fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rename { from, to, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    /// Delete a file.
    pub fn remove(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Remove { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    /// Create a directory.
    pub fn mkdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Mkdir { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    /// Upload a file — progress via a channel (non-blocking, fire-and-forget).
    /// Use `cx.spawn` to run async; the UI observes the progress channel.
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

    /// Download a file — similar to upload.
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

    /// Close the SFTP session.
    pub fn close(&self) {
        let _ = self.cmd_tx.try_send(SftpCmd::Close);
    }

    /// Subscribe to events (Ready/Error/Closed).
    pub fn subscribe(&self) -> Receiver<SftpEvent> {
        self.event_rx
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| async_channel::bounded(1).1)
    }

    /// Is SFTP still alive?
    pub fn alive(&self) -> bool {
        *self.alive.lock().unwrap()
    }
}
```

### 3.5. `sftp_task.rs` — tokio task handling SFTP commands

```rust
// crates/ssh/src/sftp_task.rs

use std::path::PathBuf;

use russh_sftp::client::SftpSession as SftpChannel;
use async_channel::{Sender, Receiver};
use tokio::sync::oneshot;

use oneterm_core::Result;

use crate::sftp::{SftpCmd, SftpEvent, FileEntry, FileStat};

/// Tokio task handling SFTP commands.
/// Runs in parallel with `ssh_main_task` on the same tokio runtime.
pub(crate) async fn sftp_task(
    sftp: SftpChannel,
    cmd_rx: Receiver<SftpCmd>,
    event_tx: Sender<SftpEvent>,
    alive: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    log::info!("sftp_task: started");
    let _ = event_tx.try_send(SftpEvent::Ready);

    loop {
        // Receive a command from the UI
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
                    .map_err(|e| oneterm_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Remove { path, reply }) => {
                let result = sftp.remove_file(&path).await
                    .map_err(|e| oneterm_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rmdir { path, reply }) => {
                let result = sftp.remove_dir(&path).await
                    .map_err(|e| oneterm_core::AppError::msg(e.to_string()));
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Mkdir { path, reply }) => {
                let result = sftp.create_dir(&path).await
                    .map_err(|e| oneterm_core::AppError::msg(e.to_string()));
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

/// Read a directory — convert russh-sftp's `DirEntry` to `FileEntry`.
async fn sftp_read_dir(
    sftp: &SftpChannel,
    path: &PathBuf,
) -> Result<Vec<FileEntry>> {
    let mut entries = sftp.read_dir(path).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;

    let mut result = Vec::new();
    while let Some(entry) = entries.next().await {
        let entry = entry.map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
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
    // Sort: folders first, then files by name.
    result.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(result)
}

/// Upload a file with progress reporting.
async fn sftp_upload(
    sftp: &SftpChannel,
    local: &PathBuf,
    remote: &PathBuf,
    progress: &Sender<f64>,
) -> Result<()> {
    let local_data = tokio::fs::read(local).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
    let total = local_data.len() as u64;

    let mut remote_file = sftp.open(remote).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;

    // Write in 32KB chunks — send progress after each chunk.
    const CHUNK: usize = 32 * 1024;
    let mut written: u64 = 0;
    for chunk in local_data.chunks(CHUNK) {
        remote_file.write_all(chunk).await
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        written += chunk.len() as u64;
        let pct = if total > 0 { written as f64 / total as f64 } else { 1.0 };
        let _ = progress.try_send(pct);
    }
    remote_file.flush().await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
    let _ = progress.try_send(1.0);
    Ok(())
}

/// Download a file with progress reporting.
async fn sftp_download(
    sftp: &SftpChannel,
    remote: &PathBuf,
    local: &PathBuf,
    progress: &Sender<f64>,
) -> Result<()> {
    let mut remote_file = sftp.open(remote).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
    let metadata = sftp.metadata(remote).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
    let total = metadata.len().unwrap_or(0);

    let mut local_file = tokio::fs::File::create(local).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;

    const CHUNK: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut read: u64 = 0;
    loop {
        let n = remote_file.read(&mut buf).await
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        if n == 0 { break; }
        local_file.write_all(&buf[..n]).await
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        read += n as u64;
        let pct = if total > 0 { read as f64 / total as f64 } else { 1.0 };
        let _ = progress.try_send(pct);
    }
    local_file.flush().await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
    let _ = progress.try_send(1.0);
    Ok(())
}

/// Get detailed metadata.
async fn sftp_stat(
    sftp: &SftpChannel,
    path: &PathBuf,
) -> Result<FileStat> {
    let metadata = sftp.metadata(path).await
        .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
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

### 3.6. Changing `session.rs` — add SFTP to `connect()`

```rust
// crates/ssh/src/session.rs — CHANGED

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
    // NEW:
    pub(crate) sftp: Mutex<Option<SftpSession>>,
}
```

In `connect()` block_on, after the shell channel is opened:

```rust
// ── NEW: try to open an SFTP channel (optional) ──────────────────────
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

// Spawn the shell task (handle moves in — the SFTP channel is already split out)
tokio::spawn(ssh_main_task(handle, channel, ...));

// The SFTP session already has cmd_tx/event_rx — a separate task was spawned
if let Some(ref sftp) = sftp_session {
    // sftp_task was already spawned inside open_sftp()
}
```

```rust
/// Open an SFTP channel on the same handle + spawn sftp_task.
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

### 3.7. New dependency

```toml
# Cargo.toml [workspace.dependencies] — ADD:
russh-sftp = "2.0"

# crates/ssh/Cargo.toml [dependencies] — ADD:
russh-sftp.workspace = true
```
---


## 4. UI integration — SftpPanel

### 4.1. Problem: global SftpPanel vs per-tab SSH session

```
Center Dock (tabs)              Right Dock
┌──────────────────────┐       ┌──────────────────┐
│ Tab1: local shell    │       │ SessionPanel     │
│ Tab2: SSH prod       │  ←→   ├──────────────────┤
│ Tab3: SSH dev        │       │ SftpPanel        │
│ Tab4: SSH staging    │       │ (1 panel for all │
└──────────────────────┘       │  tabs)           │
                               └──────────────────┘
```

SftpPanel is **a single panel** in the right dock. But there can be multiple SSH tabs
in the center dock. We need a mechanism to:

1. Know **which tab is active** in the center dock.
2. If the active tab is an SSH session **with SFTP** → SftpPanel shows the file tree.
3. If the active tab is a local shell or SSH without SFTP → SftpPanel shows
   "No SFTP connection."
4. When the user switches tabs → SftpPanel swaps to the new tab's SFTP backend.

### 4.2. Solution: AppState tracking the active SFTP

```rust
// crates/ui/src/state/app_state.rs — CHANGED

use std::sync::Arc;

pub struct AppState {
    pub dock_area: Option<WeakEntity<DockArea>>,
    // NEW: SFTP backend of the active SSH tab.
    // None = no SFTP (local shell, or an SSH server that doesn't support SFTP).
    pub active_sftp: Option<Arc<SftpSession>>,
}
```

**Flow:**
1. `connect_dialog.rs` → `ssh_connect()` succeeds → returns `SshSession`
   (with `sftp: Option<SftpSession>`)
2. `TerminalPanel::from_session()` creates the panel — if the session has SFTP:
   - Extract the `SftpSession` (or keep an `Arc` reference)
   - Set `AppState.active_sftp = Some(sftp)`
3. `TerminalPanel::set_active(true/false)` — hook when the active tab changes:
   - `set_active(true)` + session has SFTP → set `active_sftp`
   - `set_active(false)` → if `active_sftp` is this session → set `None`
4. `SftpPanel` observes `AppState.active_sftp` → re-render on change

### 4.3. How to access SFTP from TerminalPanel

**Option A: Downcast `dyn TerminalSession` to `SshSession`**

```rust
// Add a method on the TerminalSession trait:
fn sftp(&self) -> Option<Arc<SftpSession>> { None }

// SshSession impl:
fn sftp(&self) -> Option<Arc<SftpSession>> {
    self.sftp.lock().unwrap().clone()
}
```

→ Simplest. The local session returns `None` (default). The UI doesn't need to know
the concrete type, just calls `session.sftp()`.

**Option B: Separate `SftpCapable` trait**

```rust
pub trait SftpCapable {
    fn sftp(&self) -> Option<&SftpSession>;
}
```

→ More complex, needs downcast. Not recommended.

→ **Choose Option A** — add `fn sftp()` to the `TerminalSession` trait with
default `None`.

### 4.4. SftpPanel — new state

```rust
// crates/ui/src/views/sftp/file_browser.rs — CHANGED

use std::path::PathBuf;
use gpui_component::list::ListItem;

pub struct SftpPanel {
    focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    /// The SFTP session currently displayed (from the active SSH tab).
    /// None = no SFTP connection.
    sftp: Option<Arc<SftpSession>>,

    // ── File tree state ─────────────────────────────────────
    /// The directory currently displayed.
    cwd: PathBuf,
    /// List of entries in `cwd` (cache).
    entries: Vec<FileEntry>,
    /// The selected entry (single click).
    selected: Option<usize>,
    /// Loading (waiting for ReadDir response).
    loading: bool,
    /// The most recent error (shown inline).
    error: Option<String>,

    // ── Transfer queue ──────────────────────────────────────
    /// List of running/pending transfers.
    transfers: Vec<TransferItem>,

    // ── Sort state ──────────────────────────────────────────
    sort_by: SortBy,
    sort_asc: bool,
}

/// One item in the transfer queue.
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
| **Rename** | Click `✏` / F2 / context menu | Dialog to enter new name → `sftp.rename(from, to)` |
| **Delete** | Click `🗑` / Delete key | Confirm dialog → `sftp.remove(path)` |
| **Upload** | Click `⬆` / drag-drop file | File picker → `sftp.upload(local, remote)` → add to transfer queue |
| **Download** | Click `⬇` / context menu | File picker → `sftp.download(remote, local)` → add to transfer queue |
| **New Folder** | Click `📁` | Dialog to enter name → `sftp.mkdir(path)` |
| **Properties** | Click `📋` / context menu | `sftp.stat(path)` → detail dialog |
| **Open** | Double-click file | Download to temp → open local app → (edit → re-upload?) |
| **Refresh** | Click `⟳` / F5 | `sftp.read_dir(cwd)` → update `entries` |

### 4.7. Transfer queue — parallel with the terminal

```rust
// When the user clicks Upload:
fn on_upload(&mut self, local: PathBuf, cx: &mut Context<Self>) {
    let sftp = self.sftp.clone().unwrap();
    let remote = self.cwd.join(local.file_name().unwrap());

    // Add to the transfer queue (UI shows it immediately)
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

    // Start the upload (async — doesn't block the UI)
    let (progress_rx, reply_rx) = sftp.upload(local, remote);

    // Spawn a task to poll progress + update the UI
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

    // ← Meanwhile, the terminal still accepts commands normally!
    //    Because sftp_task and ssh_main_task are 2 independent tokio tasks.
}
```

### 4.8. Context menu (right-click)

```
Right-click on a file:
┌──────────────────┐
│ ⬇ Download       │
│ ✏ Rename         │
│ 🗑 Delete         │
│ ─────────────── │
│ 📋 Properties    │
└──────────────────┘

Right-click on a folder:
┌──────────────────┐
│ 📂 Open           │
│ ⬇ Download       │
│ ✏ Rename         │
│ 🗑 Delete         │
│ ─────────────── │
│ 📋 Properties    │
└──────────────────┘

Right-click on empty space:
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

| Situation | UI behavior |
|------------|-------------|
| SFTP channel fails on connect | Terminal still works. SftpPanel: "This server doesn't support SFTP." |
| ReadDir fails (permission denied) | Show error inline: "Permission denied: /root/" |
| Delete fails | Error dialog: "Cannot delete: Permission denied" |
| Upload fails (remote disk full) | Transfer queue: status = Error, message = "No space left on device" |
| Connection drops | `SftpEvent::Closed` → SftpPanel: "Connection lost." + the terminal also disconnects |

### 4.11. Changing `TerminalPanel::set_active` — swap SFTP

```rust
// crates/ui/src/views/terminal/panel.rs — CHANGED

impl Panel for TerminalPanel {
    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }

        // NEW: when the active tab changes → update AppState.active_sftp
        if active {
            // This tab becomes active → extract SFTP from the session (if any)
            let sftp = self.view.read(cx).session.read(cx).sftp();
            AppState::global(cx).update(cx, |state, cx| {
                state.active_sftp = sftp;
                cx.notify();
            });
        }
        // No need to set None on deactivate — the new active tab will overwrite.
    }
}
```

### 4.12. SftpPanel observes AppState

```rust
// crates/ui/src/views/sftp/file_browser.rs

impl SftpPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // Observe AppState — when active_sftp changes, update + re-render
        let app_state = AppState::global(cx);
        cx.observe(&app_state, |this, state, cx| {
            let new_sftp = state.active_sftp.clone();
            if this.sftp.as_ref().map(|s| s.as_ref()) != new_sftp.as_ref().map(|s| s.as_ref()) {
                this.sftp = new_sftp;
                // Reset state when the session changes
                this.entries.clear();
                this.selected = None;
                this.error = None;
                // Load the root dir of the new session
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

### 4.13. File layout changes

```
crates/ui/src/views/sftp/
├── mod.rs              # Re-export SftpPanel + submodules
├── file_browser.rs     # SftpPanel (Panel + Render) — state + main render
├── file_list.rs        # NEW — render file list (entries, sort, select)
├── toolbar.rs          # NEW — render breadcrumb + action toolbar
├── transfer_queue.rs   # NEW — render transfer queue (progress bars)
├── context_menu.rs     # NEW — context menu for file/folder/empty area
└── detail_dialog.rs    # NEW — file/folder properties dialog
```
---


## 5. Implementation roadmap

### 5.1. Dependency graph between steps

```
Step 1: Dependency + types
  │
  ├──→ Step 2: SFTP backend (sftp.rs + sftp_task.rs)
  │      │
  │      └──→ Step 3: Modify connect() — open SFTP channel
  │             │
  │             ├──→ Step 4: TerminalSession trait — add fn sftp()
  │             │
  │             └──→ Step 5: AppState — track active_sftp
  │                    │
  │                    ├──→ Step 6: SftpPanel — file tree + navigation
  │                    │
  │                    ├──→ Step 7: SftpPanel — file operations
  │                    │
  │                    ├──→ Step 8: SftpPanel — transfer queue
  │                    │
  │                    └──→ Step 9: SftpPanel — detail dialog + context menu
```

### 5.2. Detailed checklist

#### Step 1: Dependency + core types

- [x] Add `russh-sftp = "2.0"` to `Cargo.toml` `[workspace.dependencies]` *(using 2.3)*
- [x] Add `russh-sftp.workspace = true` to `crates/ssh/Cargo.toml`
- [x] Create `crates/ssh/src/sftp.rs` — `SftpCmd`, `SftpEvent`, `FileEntry`, `FileStat`
- [x] Add `pub mod sftp;` + re-export in `crates/ssh/src/lib.rs`
- [x] `cargo build` passes

#### Step 2: SFTP backend — sftp.rs + sftp_task.rs

- [x] Implement `SftpSession` struct (cmd_tx, event_rx, alive)
- [x] Implement sync methods: `read_dir`, `stat`, `rename`, `remove`, `mkdir`
- [x] Implement async methods: `upload`, `download` (return progress + reply channels)
- [x] Create `crates/ssh/src/sftp_task.rs` — `sftp_task()` tokio task
- [x] Implement `sftp_read_dir`, `sftp_stat`, `sftp_upload`, `sftp_download` helpers
- [x] Add `pub mod sftp_task;` in `lib.rs`
- [x] `cargo build` passes

#### Step 3: Modify connect() — open SFTP channel

- [x] Add `sftp: Mutex<Option<SftpSession>>` field to the `SshSession` struct
- [x] Write the `open_sftp()` async helper — open channel + request subsystem + spawn task
- [x] In `connect()` block_on: call `open_sftp(&handle)` after the shell channel is opened
- [x] Handle `Err` → `sftp = None` (terminal still works)
- [x] Spawn `sftp_task` (inside `open_sftp`)
- [x] Set the `sftp` field in the `SshSession` return value
- [x] `cargo build` passes
- [x] `cargo run` — connect SSH, check the log "SFTP channel opened"

#### Step 4: TerminalSession trait — add fn sftp()

- [x] Add method `fn sftp(&self) -> Option<Arc<dyn SftpBackend>>` to `TerminalSession`
  - Default implementation: `None`
- [x] Implement for `SshSession`: return `self.sftp.lock().unwrap().clone()`
  - Changed `Mutex<Option<SftpSession>>` → `Mutex<Option<Arc<SftpSession>>>`
- [x] `LocalSession` — no impl needed (uses default `None`)
- [x] Import `SftpBackend` into the `core` crate — define the trait in `crates/core/src/sftp.rs`
  - **Solution**: Define the `SftpBackend` trait in `core`, impl for `SftpSession` in the `ssh` crate.
- [x] `cargo build` passes + `cargo test` passes

#### Step 5: AppState — track active_sftp

- [x] Add `active_sftp: Option<Arc<dyn SftpBackend>>` to `AppState`
- [x] In `TerminalPanel::set_active(true)`:
  - Call `session.sftp()` → set `AppState.active_sftp`
- [x] `cargo build` passes

#### Step 6: SftpPanel — file tree + navigation

- [x] Change the `SftpPanel` struct: add state fields (sftp, cwd, entries, selected, loading, error)
- [x] Observe `AppState` — when `active_sftp` changes → update + load root dir
- [x] Implement `load_dir(path)` — call `sftp.read_dir()` inside `cx.spawn`
- [x] Render breadcrumb (path + `↑` parent + `⟳` refresh)
- [x] Render file list (folders first, then files, icon + name + size + date)
- [x] Double-click folder → navigate
- [x] Single-click → select
- [x] Render "No SFTP connection." when `sftp = None`
- [x] Render "Loading..." when `loading = true`
- [x] Render error inline when `error = Some(...)`
- [x] `cargo build` passes + manual test

#### Step 7: SftpPanel — file operations

- [x] Toolbar buttons: Upload, Download, Rename, Delete, New Folder, Properties
- [x] Rename: dialog (InputState) → `sftp.rename()`
- [x] Delete: confirm dialog → `sftp.remove()` / `sftp.rmdir()`
- [x] New Folder: dialog → `sftp.mkdir()`
- [x] Upload: file picker dialog → `sftp.upload()` → add to transfer queue
- [x] Download: file picker dialog → `sftp.download()` → add to transfer queue
- [x] Refresh the file list after each successful operation
- [x] Error handling: show a dialog/toast when an operation fails
- [x] `cargo build` passes + manual test

#### Step 8: SftpPanel — transfer queue

- [x] `TransferItem` struct (direction, filename, progress, status, error)
- [x] Render the transfer queue section (below the action toolbar)
- [x] Progress bar for each transfer (0% – 100%)
- [x] Status icon: ⬆/⬇ + ✓/✗/⏳
- [x] Spawn a task to poll the progress channel → update the UI
- [x] Clear completed transfers (button or auto-remove after 5s)
- [x] Cancel a transfer — `tokio_util::CancellationToken` + spawn upload/download as a separate task, Cancel button (Close icon) in the transfer queue
- [x] `cargo build` passes + manual test (upload a large file + type terminal commands)

#### Step 9: SftpPanel — detail dialog + context menu

- [x] Context menu for a file: Download, Rename, Delete, Properties
- [x] Context menu for a folder: Open, Rename, Delete, Properties (no Download — folder download was blocked from step 7)
- [x] Context menu for empty area: Upload, New Folder, Refresh
- [x] Detail dialog: `sftp.stat()` → show size, modified, permissions, uid, gid, accessed, owner, group, path
- [x] Permissions shown as `rwxr-xr-x (0775)`
- [x] `cargo build` passes + manual test

### 5.3. SFTP trait abstraction (resolving core ↔ ssh)

**Problem:** the `core` crate is a leaf (doesn't depend on `ssh`). It can't import
`SftpSession` from `ssh` into `core`.

**Solution:** define an abstract trait in `core`:

```rust
// crates/core/src/sftp.rs (NEW)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

/// Abstract SFTP backend — implemented by the `ssh` crate.
/// The UI uses it via `dyn SftpBackend`, unaware of `russh-sftp`.
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
// crates/ssh/src/sftp.rs — impl SftpBackend for SftpSession
impl oneterm_core::SftpBackend for SftpSession {
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> { ... }
    // ...
}

// crates/core/src/terminal/session.rs — add to the TerminalSession trait:
fn sftp(&self) -> Option<Arc<dyn SftpBackend>> { None }
```

### 5.4. Final file layout

```
crates/
├── core/src/
│   ├── sftp.rs                 # NEW — SftpBackend trait + FileEntry + FileStat
│   ├── terminal/session.rs     # Add: fn sftp() -> Option<Arc<dyn SftpBackend>>
│   └── lib.rs                  # Add: pub mod sftp; pub use sftp::{SftpBackend, FileEntry, FileStat}
│
├── ssh/src/
│   ├── sftp.rs                 # NEW — SftpSession + impl SftpBackend
│   ├── sftp_task.rs            # NEW — tokio task
│   ├── session.rs              # Add: sftp field + open_sftp()
│   ├── session_terminal.rs     # Add: fn sftp() impl
│   └── lib.rs                  # Add: pub mod sftp; pub mod sftp_task
│
└── ui/src/
    ├── state/app_state.rs      # Add: active_sftp field
    ├── views/terminal/panel.rs # Add: set_active → update active_sftp
    └── views/sftp/
        ├── mod.rs              # Re-export + submodules
        ├── file_browser.rs     # SftpPanel — state + main render
        ├── file_list.rs        # NEW — file list rendering
        ├── toolbar.rs          # NEW — breadcrumb + action toolbar
        ├── transfer_queue.rs   # NEW — transfer queue rendering
        ├── context_menu.rs     # NEW — context menu
        └── detail_dialog.rs    # NEW — properties dialog
```

### 5.5. Priority order

| Phase | Step | Goal | Estimate |
|-------|------|----------|----------|
| **Phase 1: Backend** | 1–3 | SFTP channel works, `read_dir` returns data | |
| **Phase 2: Wiring** | 4–5 | UI can access the SFTP backend via a trait | |
| **Phase 3: MVP UI** | 6 | File tree shows, navigation works | |
| **Phase 4: Operations** | 7 | Rename/delete/upload/download work | |
| **Phase 5: Polish** | 8–9 | Transfer queue + detail dialog + context menu | |

### 5.6. Risks & solutions

| Risk | Solution |
|--------|-----------|
| `russh-sftp` API doesn't match the design | Read the `russh-sftp` source on crates.io/docs.rs before implementing |
| `Handle` doesn't allow opening a channel after spawn | Open the SFTP channel in `block_on` (before spawn) — already designed |
| The `core` crate can't import `ssh` types | Use the `SftpBackend` trait in `core`, impl in `ssh` |
| Large file upload/download blocks the UI | Stream chunks + progress channel + `cx.spawn` (non-blocking) |
| Many SSH tabs — SftpPanel shows the wrong session | `AppState.active_sftp` + `set_active` hook |
| Connection drops mid-transfer | `SftpEvent::Closed` → mark the transfer as Error |
| Drag-drop a file into the panel | GPUI `on_drop` handler — check the API in the reference |

### 5.7. References

| Need | Read |
|-----|-----|
| russh channel API | `russh` docs.rs — `client::Handle`, `Channel`, `request_subsystem` |
| russh-sftp client API | `russh-sftp` docs.rs — `SftpSession`, `read_dir`, `metadata`, `open` |
| gpui-component Tree | `reference/gpui-component/crates/ui/src/tree/` |
| gpui-component List | `reference/gpui-component/crates/ui/src/list/` |
| gpui-component Dialog | `reference/gpui-component/crates/ui/src/dialog/` |
| gpui-component ContextMenu | `reference/gpui-component/crates/ui/src/menu/` |
| gpui-component Button | `reference/gpui-component/crates/ui/src/button/` |
| gpui-component Input | `reference/gpui-component/crates/ui/src/input/` |
| Cmd/SshListener pattern | `crates/ssh/src/listener.rs` (current) |
| ssh_main_task pattern | `crates/ssh/src/task.rs` (current) |
| SshSession::connect | `crates/ssh/src/session.rs` (current) |
| connect_dialog flow | `crates/ui/src/views/session_tabs/connect_dialog.rs` (current) |
| TerminalPanel set_active | `crates/ui/src/views/terminal/panel.rs` (current) |
---