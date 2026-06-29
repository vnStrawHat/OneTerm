//! SFTP types — commands, events, file entries.
//!
//! Được gửi giữa UI thread (sync) và tokio task (async) qua `async_channel`.
//! Tương tự pattern `Cmd`/`SessionEvent` trong `listener.rs`.
//!
//! `FileEntry`, `FileStat` định nghĩa trong `core` crate (leaf crate, không
//! phụ thuộc `ssh`). `SftpBackend` trait cũng trong `core`. `SftpSession`
//! implement `SftpBackend` ở đây.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender};
use tokio::sync::oneshot;

use myterm2_core::{FileEntry, FileStat, Result, SftpBackend};

// ── Re-export từ core ────────────────────────────────────────
// FileEntry, FileStat đã định nghĩa trong core, re-export cho tiện.

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
    /// Upload file local → remote. Progress qua `progress` (0.0–1.0).
    /// Reply qua `async_channel::Sender` (không dùng oneshot để match trait).
    /// `transfer_id` dùng để cancel — UI gửi `SftpCmd::Cancel { transfer_id }`.
    Upload {
        transfer_id: u64,
        local: PathBuf,
        remote: PathBuf,
        progress: Sender<f64>,
        reply: Sender<Result<()>>,
    },
    /// Download file remote → local. Progress qua `progress` (0.0–1.0).
    /// Reply qua `async_channel::Sender`.
    Download {
        transfer_id: u64,
        remote: PathBuf,
        local: PathBuf,
        progress: Sender<f64>,
        reply: Sender<Result<()>>,
    },
    /// Hủy transfer đang chạy (upload/download).
    /// `transfer_id` phải khớp với ID đã gửi trong Upload/Download.
    Cancel { transfer_id: u64 },
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
///
/// Tương tự `SshSession` pattern: UI gọi sync, tokio task xử lý async.
/// `SftpSession` là handle mà UI giữ; `sftp_task` chạy nền trong tokio.
pub struct SftpSession {
    /// Channel gửi `SftpCmd` tới tokio task.
    cmd_tx: Sender<SftpCmd>,
    /// Channel nhận `SftpEvent` từ tokio task (UI subscribe).
    /// `Mutex<Option<...>>` — chỉ lấy 1 lần khi subscribe.
    event_rx: Mutex<Option<Receiver<SftpEvent>>>,
    /// SFTP có alive không (channel chưa đóng).
    alive: Arc<Mutex<bool>>,
}

impl SftpSession {
    /// Tạo `SftpSession` từ channels đã được thiết lập bởi `sftp_task`.
    /// Dùng `Arc` để share giữa nhiều panel (terminal + sftp browser).
    pub(crate) fn new(
        cmd_tx: Sender<SftpCmd>,
        event_rx: Receiver<SftpEvent>,
        alive: Arc<Mutex<bool>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            cmd_tx,
            event_rx: Mutex::new(Some(event_rx)),
            alive,
        })
    }

    // ── Lifecycle ────────────────────────────────────────────

    /// Subscribe event (Ready/Error/Closed). Chỉ lấy được 1 lần.
    pub fn subscribe(&self) -> Option<Receiver<SftpEvent>> {
        self.event_rx.lock().unwrap().take()
    }
}

// ── impl SftpBackend cho SftpSession ─────────────────────────
//
// `SftpBackend` trait định nghĩa trong `core` crate.
// `SftpSession` implement tại đây — bridge sync (UI) → async (tokio task).
// Sync methods gửi `SftpCmd` qua channel, `sftp_task` xử lý async.

impl SftpBackend for SftpSession {
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::ReadDir { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn stat(&self, path: PathBuf) -> Result<FileStat> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Stat { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rename {
                from,
                to,
                reply: tx,
            })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn remove(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Remove { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn rmdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rmdir { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn mkdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Mkdir { path, reply: tx })
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| myterm2_core::AppError::msg(e.to_string()))?
    }

    fn upload(
        &self,
        transfer_id: u64,
        local: PathBuf,
        remote: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>) {
        let (progress_tx, progress_rx) = async_channel::bounded(100);
        let (reply_tx, reply_rx) = async_channel::bounded(1);
        let _ = self.cmd_tx.try_send(SftpCmd::Upload {
            transfer_id,
            local,
            remote,
            progress: progress_tx,
            reply: reply_tx,
        });
        (progress_rx, reply_rx)
    }

    fn download(
        &self,
        transfer_id: u64,
        remote: PathBuf,
        local: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>) {
        let (progress_tx, progress_rx) = async_channel::bounded(100);
        let (reply_tx, reply_rx) = async_channel::bounded(1);
        let _ = self.cmd_tx.try_send(SftpCmd::Download {
            transfer_id,
            remote,
            local,
            progress: progress_tx,
            reply: reply_tx,
        });
        (progress_rx, reply_rx)
    }

    fn cancel_transfer(&self, transfer_id: u64) {
        log::info!("SftpSession: cancel transfer #{transfer_id}");
        let _ = self.cmd_tx.try_send(SftpCmd::Cancel { transfer_id });
    }

    fn close(&self) {
        let _ = self.cmd_tx.try_send(SftpCmd::Close);
    }

    fn alive(&self) -> bool {
        *self.alive.lock().unwrap()
    }
}
