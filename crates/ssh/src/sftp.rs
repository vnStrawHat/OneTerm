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

use oneterm_core::{FileEntry, FileStat, Result, SftpBackend, SftpSessionId};

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

// ── SftpSession — async UI ↔ Tokio task bridge ───────────────

/// SFTP session — sends commands over a channel, receives events over a channel.
///
/// Similar to the `SshSession` pattern: object-safe futures bridge UI operations
/// to the Tokio task. `SftpSession` is the handle the UI holds; `sftp_task` runs
/// in the background within Tokio.
pub struct SftpSession {
    /// Stable process-local identity used by per-session UI state.
    id: SftpSessionId,
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
            id: SftpSessionId::next(),
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
// `SftpSession` implements it here with object-safe futures. Methods enqueue
// commands asynchronously; `sftp_task` performs the protocol I/O.

impl SftpBackend for SftpSession {
    fn session_id(&self) -> SftpSessionId {
        self.id
    }

    fn read_dir(&self, path: PathBuf) -> oneterm_core::SftpFuture<'_, Vec<FileEntry>> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::ReadDir { path, reply: tx })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
    }

    fn stat(&self, path: PathBuf) -> oneterm_core::SftpFuture<'_, FileStat> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::Stat { path, reply: tx })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
    }

    fn rename(&self, from: PathBuf, to: PathBuf) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::Rename {
                    from,
                    to,
                    reply: tx,
                })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
    }

    fn remove(&self, path: PathBuf) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::Remove { path, reply: tx })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
    }

    fn rmdir(&self, path: PathBuf) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::Rmdir { path, reply: tx })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
    }

    fn mkdir(&self, path: PathBuf) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.cmd_tx
                .send(SftpCmd::Mkdir { path, reply: tx })
                .await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?;
            rx.await
                .map_err(|error| oneterm_core::AppError::msg(error.to_string()))?
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> (Arc<SftpSession>, Receiver<SftpCmd>) {
        let (cmd_tx, cmd_rx) = async_channel::bounded(1);
        let (_event_tx, event_rx) = async_channel::bounded(1);
        let session = SftpSession::new(cmd_tx, event_rx, Arc::new(Mutex::new(true)));
        (session, cmd_rx)
    }

    #[tokio::test]
    async fn read_dir_awaits_the_protocol_reply() {
        let (session, cmd_rx) = test_session();
        tokio::spawn(async move {
            match cmd_rx.recv().await.expect("command must arrive") {
                SftpCmd::ReadDir { path, reply } => {
                    assert_eq!(path, PathBuf::from("/tmp"));
                    reply.send(Ok(Vec::new())).expect("reply must be received");
                }
                _ => panic!("unexpected command"),
            }
        });

        let entries = session
            .read_dir(PathBuf::from("/tmp"))
            .await
            .expect("read_dir must succeed");
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn metadata_operation_reports_a_closed_command_transport() {
        let (session, cmd_rx) = test_session();
        drop(cmd_rx);

        let error = session
            .stat(PathBuf::from("/tmp"))
            .await
            .expect_err("closed transport must fail");
        assert!(error.to_string().contains("closed"));
    }
    #[test]
    fn session_identity_is_stable_across_clones_and_unique_per_session() {
        let (first, _first_rx) = test_session();
        let first_clone = Arc::clone(&first);
        let (second, _second_rx) = test_session();

        assert_eq!(first.session_id(), first_clone.session_id());
        assert_ne!(first.session_id(), second.session_id());
    }
}
