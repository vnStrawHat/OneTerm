//! SFTP types — commands, events, file entries.
//!
//! Sent between the UI thread (sync) and the tokio task (async) via
//! `async_channel`. Similar to the `Cmd`/`SessionEvent` pattern in `listener.rs`.
//!
//! `FileEntry`, `FileStat` are defined in the `core` crate (a leaf crate that
//! does not depend on `ssh`). The `SftpBackend` trait is also in `core`.
//! `SftpSession` implements `SftpBackend` here.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_channel::{Receiver, Sender};
use tokio::sync::oneshot;

use oneterm_core::{FileEntry, FileStat, Result, SftpBackend};

// ── Re-export from core ──────────────────────────────────────
// FileEntry, FileStat are defined in core; re-exported here for convenience.

// ── SFTP commands: UI → tokio task ───────────────────────────

/// SFTP command sent from the UI thread to the tokio task via `async_channel`.
pub enum SftpCmd {
    /// Read a directory → returns the list of entries.
    ReadDir {
        path: PathBuf,
        reply: oneshot::Sender<Result<Vec<FileEntry>>>,
    },
    /// Get metadata for a single file/folder.
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
    /// Remove a file.
    Remove {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Remove an empty directory.
    Rmdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Create a directory.
    Mkdir {
        path: PathBuf,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Upload a local file → remote. Progress via `progress` (0.0–1.0).
    /// Reply via `async_channel::Sender` (not oneshot, to match the trait).
    /// `transfer_id` is used to cancel — the UI sends `SftpCmd::Cancel { transfer_id }`.
    Upload {
        transfer_id: u64,
        local: PathBuf,
        remote: PathBuf,
        progress: Sender<f64>,
        reply: Sender<Result<()>>,
    },
    /// Download a remote file → local. Progress via `progress` (0.0–1.0).
    /// Reply via `async_channel::Sender`.
    Download {
        transfer_id: u64,
        remote: PathBuf,
        local: PathBuf,
        progress: Sender<f64>,
        reply: Sender<Result<()>>,
    },
    /// Cancel a running transfer (upload/download).
    /// `transfer_id` must match the ID sent in Upload/Download.
    Cancel { transfer_id: u64 },
    /// Close the SFTP session.
    Close,
}

// ── SFTP event: tokio task → UI ──────────────────────────────

/// Event sent from the SFTP task to the UI (via `async_channel`).
#[derive(Debug, Clone)]
pub enum SftpEvent {
    /// SFTP session is ready (after the handshake).
    Ready,
    /// SFTP session errored/disconnected.
    Error(String),
    /// SFTP session is closed.
    Closed,
}

// ── SftpSession — bridge sync (UI) ↔ async (tokio task) ──────

/// SFTP session — sends commands over a channel, receives events over a channel.
///
/// Similar to the `SshSession` pattern: the UI calls sync, the tokio task handles
/// async. `SftpSession` is the handle the UI holds; `sftp_task` runs in the
/// background within tokio.
pub struct SftpSession {
    /// Channel sending `SftpCmd` to the tokio task.
    cmd_tx: Sender<SftpCmd>,
    /// Channel receiving `SftpEvent` from the tokio task (UI subscribes).
    /// `Mutex<Option<...>>` — taken only once on subscribe.
    event_rx: Mutex<Option<Receiver<SftpEvent>>>,
    /// Whether SFTP is alive (channel not yet closed).
    alive: Arc<Mutex<bool>>,
}

impl SftpSession {
    /// Create an `SftpSession` from channels already set up by `sftp_task`.
    /// Uses `Arc` to share across multiple panels (terminal + sftp browser).
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

    /// Subscribe to events (Ready/Error/Closed). Can be taken only once.
    pub fn subscribe(&self) -> Option<Receiver<SftpEvent>> {
        self.event_rx.lock().unwrap().take()
    }
}

// ── impl SftpBackend for SftpSession ─────────────────────────
//
// The `SftpBackend` trait is defined in the `core` crate.
// `SftpSession` implements it here — bridging sync (UI) → async (tokio task).
// Sync methods send `SftpCmd` over the channel; `sftp_task` handles it async.

impl SftpBackend for SftpSession {
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::ReadDir { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    fn stat(&self, path: PathBuf) -> Result<FileStat> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Stat { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rename {
                from,
                to,
                reply: tx,
            })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    fn remove(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Remove { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    fn rmdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Rmdir { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
    }

    fn mkdir(&self, path: PathBuf) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .try_send(SftpCmd::Mkdir { path, reply: tx })
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?;
        rx.blocking_recv()
            .map_err(|e| oneterm_core::AppError::msg(e.to_string()))?
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
