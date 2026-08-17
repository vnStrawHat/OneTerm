//! SFTP types — commands, events, file entries.
//!
//! Sent between the UI thread (sync) and the tokio task (async) via
//! `async_channel`. Similar to the `Cmd`/`SessionEvent` pattern in `listener.rs`.
//!
//! `FileEntry`, `FileStat`, `RemotePath` are defined in the `core` crate (a leaf
//! crate that does not depend on `ssh`). The `SftpBackend` trait is also in
//! `core`. `SftpSession` implements `SftpBackend` here.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_channel::Sender;
use tokio::sync::oneshot;

use oneterm_core::{
    AppError, FileEntry, FileStat, RemotePath, Result, SftpBackend, SftpSessionId, TransferEvent,
    TransferHandle,
};

// ── SFTP commands: UI → tokio task ───────────────────────────

/// SFTP command sent from the UI thread to the tokio task via `async_channel`.
pub(crate) enum SftpCmd {
    /// Read a directory → returns the list of entries.
    ReadDir {
        path: RemotePath,
        reply: oneshot::Sender<Result<Vec<FileEntry>>>,
    },
    /// Get metadata for a single file/folder.
    Stat {
        path: RemotePath,
        reply: oneshot::Sender<Result<FileStat>>,
    },
    /// Rename a file/folder.
    Rename {
        from: RemotePath,
        to: RemotePath,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Remove a file.
    Remove {
        path: RemotePath,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Remove a directory tree (bounded, symlinks are unlinked, never followed).
    RemoveDirAll {
        path: RemotePath,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Create a directory.
    Mkdir {
        path: RemotePath,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Upload a local file → remote. Progress via `events`.
    /// Reply via `async_channel::Sender` (not oneshot, to match the trait).
    /// `transfer_id` is used to cancel — the UI sends `SftpCmd::Cancel { transfer_id }`.
    Upload {
        transfer_id: u64,
        local: PathBuf,
        remote: RemotePath,
        events: Sender<TransferEvent>,
        reply: Sender<Result<()>>,
    },
    /// Download a remote file → local. Progress via `events`.
    /// Reply via `async_channel::Sender`.
    Download {
        transfer_id: u64,
        remote: RemotePath,
        local: PathBuf,
        events: Sender<TransferEvent>,
        reply: Sender<Result<()>>,
    },
    /// Cancel a running transfer (upload/download).
    /// `transfer_id` must match the ID sent in Upload/Download.
    Cancel { transfer_id: u64 },
    /// Close the SFTP session.
    Close,
}

// ── SFTP event: tokio task → UI ──────────────────────────────

/// Lifecycle event emitted by the SFTP task (via `async_channel`). No UI
/// consumer subscribes today — `SftpBackend::alive()` is the observable state
/// — so the receiver side is dropped after setup.
#[derive(Debug, Clone)]
pub(crate) enum SftpEvent {
    /// SFTP session is ready (after the handshake).
    Ready,
    /// SFTP session is closed.
    Closed,
}

// ── SftpSession — async UI ↔ Tokio task bridge ───────────────

/// SFTP session — sends commands over a channel, receives events over a channel.
///
/// Similar to the `SshSession` pattern: object-safe futures bridge UI operations
/// to the Tokio task. `SftpSession` is the handle the UI holds; `sftp_task` runs
/// in the background within Tokio.
pub(crate) struct SftpSession {
    /// Stable process-local identity used by per-session UI state.
    id: SftpSessionId,
    /// Channel sending `SftpCmd` to the tokio task.
    cmd_tx: Sender<SftpCmd>,
    /// Whether SFTP is alive (task still running; cleared when the task exits,
    /// including when the SSH connection ends — ARCH-28).
    alive: Arc<Mutex<bool>>,
}

impl SftpSession {
    /// Create an `SftpSession` from the command channel set up for `sftp_task`.
    /// Uses `Arc` to share across multiple panels (terminal + sftp browser).
    pub(crate) fn new(cmd_tx: Sender<SftpCmd>, alive: Arc<Mutex<bool>>) -> Arc<Self> {
        Arc::new(Self {
            id: SftpSessionId::next(),
            cmd_tx,
            alive,
        })
    }

    /// Enqueue a command carrying a oneshot reply and await that reply.
    async fn request<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<Result<T>>) -> SftpCmd,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(build(tx))
            .await
            .map_err(|error| AppError::msg(error.to_string()))?;
        rx.await.map_err(|error| AppError::msg(error.to_string()))?
    }

    /// Enqueue a transfer command; a failed enqueue is reported through the handle.
    fn start_transfer(
        &self,
        transfer_id: u64,
        kind: &'static str,
        build: impl FnOnce(Sender<TransferEvent>, Sender<Result<()>>) -> SftpCmd,
    ) -> TransferHandle {
        let (events_tx, events) = async_channel::bounded(100);
        let (reply_tx, result) = async_channel::bounded(1);
        if let Err(error) = self.cmd_tx.try_send(build(events_tx, reply_tx)) {
            log::warn!("SftpSession: failed to enqueue {kind} #{transfer_id}: {error}");
            return TransferHandle::failed(AppError::msg(format!(
                "failed to enqueue {kind}: {error}"
            )));
        }
        TransferHandle { events, result }
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

    fn read_dir(&self, path: RemotePath) -> oneterm_core::SftpFuture<'_, Vec<FileEntry>> {
        Box::pin(self.request(|reply| SftpCmd::ReadDir { path, reply }))
    }

    fn stat(&self, path: RemotePath) -> oneterm_core::SftpFuture<'_, FileStat> {
        Box::pin(self.request(|reply| SftpCmd::Stat { path, reply }))
    }

    fn rename(&self, from: RemotePath, to: RemotePath) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(self.request(|reply| SftpCmd::Rename { from, to, reply }))
    }

    fn remove(&self, path: RemotePath) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(self.request(|reply| SftpCmd::Remove { path, reply }))
    }

    fn remove_dir_all(&self, path: RemotePath) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(self.request(|reply| SftpCmd::RemoveDirAll { path, reply }))
    }

    fn mkdir(&self, path: RemotePath) -> oneterm_core::SftpFuture<'_, ()> {
        Box::pin(self.request(|reply| SftpCmd::Mkdir { path, reply }))
    }

    fn upload(&self, transfer_id: u64, local: PathBuf, remote: RemotePath) -> TransferHandle {
        self.start_transfer(transfer_id, "upload", |events, reply| SftpCmd::Upload {
            transfer_id,
            local,
            remote,
            events,
            reply,
        })
    }

    fn download(&self, transfer_id: u64, remote: RemotePath, local: PathBuf) -> TransferHandle {
        self.start_transfer(transfer_id, "download", |events, reply| SftpCmd::Download {
            transfer_id,
            remote,
            local,
            events,
            reply,
        })
    }

    fn cancel_transfer(&self, transfer_id: u64) {
        log::info!("SftpSession: cancel transfer #{transfer_id}");
        if let Err(error) = self.cmd_tx.try_send(SftpCmd::Cancel { transfer_id }) {
            log::warn!("SftpSession: failed to enqueue cancel #{transfer_id}: {error}");
        }
    }

    fn close(&self) {
        if let Err(error) = self.cmd_tx.try_send(SftpCmd::Close) {
            log::warn!("SftpSession: failed to enqueue close: {error}");
        }
    }

    fn alive(&self) -> bool {
        *self.alive.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> (Arc<SftpSession>, async_channel::Receiver<SftpCmd>) {
        let (cmd_tx, cmd_rx) = async_channel::bounded(1);
        let session = SftpSession::new(cmd_tx, Arc::new(Mutex::new(true)));
        (session, cmd_rx)
    }

    #[tokio::test]
    async fn read_dir_awaits_the_protocol_reply() {
        let (session, cmd_rx) = test_session();
        tokio::spawn(async move {
            match cmd_rx.recv().await.expect("command must arrive") {
                SftpCmd::ReadDir { path, reply } => {
                    assert_eq!(path, RemotePath::new("/tmp"));
                    reply.send(Ok(Vec::new())).expect("reply must be received");
                }
                _ => panic!("unexpected command"),
            }
        });

        let entries = session
            .read_dir(RemotePath::new("/tmp"))
            .await
            .expect("read_dir must succeed");
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn metadata_operation_reports_a_closed_command_transport() {
        let (session, cmd_rx) = test_session();
        drop(cmd_rx);

        let error = session
            .stat(RemotePath::new("/tmp"))
            .await
            .expect_err("closed transport must fail");
        assert!(error.to_string().contains("closed"));
    }

    #[tokio::test]
    async fn transfer_enqueue_failure_reaches_reply_channel() {
        let (session, cmd_rx) = test_session();
        drop(cmd_rx);

        let handle = session.upload(7, PathBuf::from("local"), RemotePath::new("remote"));
        let error = handle
            .result
            .recv()
            .await
            .expect("enqueue failure must produce a reply")
            .expect_err("closed command transport must fail the transfer");
        assert!(error.to_string().contains("enqueue upload"));
    }

    #[tokio::test]
    async fn remove_dir_all_is_sent_as_a_recursive_delete_command() {
        let (session, cmd_rx) = test_session();
        tokio::spawn(async move {
            match cmd_rx.recv().await.expect("command must arrive") {
                SftpCmd::RemoveDirAll { path, reply } => {
                    assert_eq!(path.as_str(), "/tmp/tree");
                    reply.send(Ok(())).expect("reply must be received");
                }
                _ => panic!("unexpected command"),
            }
        });

        session
            .remove_dir_all(RemotePath::new("/tmp\\tree"))
            .await
            .expect("remove_dir_all must succeed");
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
