//! SFTP abstraction — trait + types shared by the UI and the backend.
//!
//! `core` is a leaf crate (does not depend on `ssh`). The `SftpBackend` trait defines
//! the abstract interface; the `ssh` crate implements it for `SftpSession`.
//!
//! The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use async_channel::Receiver;

use crate::Result;

// ── File entry for UI rendering ──────────────────────────────

/// A single entry in a directory (file or folder).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub permissions: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Owner name (resolved from /etc/passwd). None if it cannot be resolved.
    pub owner: Option<String>,
    /// Group name (resolved from /etc/group). None if it cannot be resolved.
    pub group: Option<String>,
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
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

/// SFTP table presentation state persisted with the dock document.
#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SftpTableState {
    #[serde(default)]
    pub column_widths: HashMap<String, f32>,
    #[serde(default)]
    pub column_visibility: HashMap<String, bool>,
}

/// Stable process-local identity for one SFTP backend instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SftpSessionId(u64);

impl SftpSessionId {
    /// Allocate a new unique session identity.
    pub fn next() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("SFTP session identity space exhausted");
        Self(id)
    }
}

// ── SftpBackend trait ────────────────────────────────────────

/// Boxed SFTP operation future used by the object-safe backend contract.
pub type SftpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Abstract SFTP backend — implemented by the `ssh` crate.
///
/// The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.
/// Metadata and mutation operations are asynchronous and never block the UI thread.
pub trait SftpBackend: Send + Sync + 'static {
    /// Stable identity for this backend instance, used to restore per-session UI state.
    fn session_id(&self) -> SftpSessionId;
    /// Read a directory — returns the list of entries.
    fn read_dir(&self, path: PathBuf) -> SftpFuture<'_, Vec<FileEntry>>;

    /// Get detailed metadata.
    fn stat(&self, path: PathBuf) -> SftpFuture<'_, FileStat>;

    /// Rename a file/folder.
    fn rename(&self, from: PathBuf, to: PathBuf) -> SftpFuture<'_, ()>;

    /// Remove a file.
    fn remove(&self, path: PathBuf) -> SftpFuture<'_, ()>;

    /// Remove an empty directory.
    fn rmdir(&self, path: PathBuf) -> SftpFuture<'_, ()>;

    /// Create a directory.
    fn mkdir(&self, path: PathBuf) -> SftpFuture<'_, ()>;

    /// Upload a file from local → remote.
    /// `transfer_id` — a unique ID created by the UI, used to cancel.
    /// Returns a progress channel (0.0–1.0) and a reply channel (Result<()>).
    /// The UI spawns a task to poll progress — it does not block the UI thread.
    fn upload(
        &self,
        transfer_id: u64,
        local: PathBuf,
        remote: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>);

    /// Download a file from remote → local.
    /// Same as upload.
    fn download(
        &self,
        transfer_id: u64,
        remote: PathBuf,
        local: PathBuf,
    ) -> (Receiver<f64>, Receiver<Result<()>>);

    /// Cancel a running transfer (upload/download).
    /// `transfer_id` must match the ID passed to `upload`/`download`.
    fn cancel_transfer(&self, transfer_id: u64);

    /// Close the SFTP session.
    fn close(&self);

    /// Is the SFTP session still alive?
    fn alive(&self) -> bool;
}
