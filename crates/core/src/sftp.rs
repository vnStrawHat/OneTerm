//! SFTP abstraction — trait + types shared by the UI and the backend.
//!
//! `core` is a leaf crate (does not depend on `ssh`). The `SftpBackend` trait defines
//! the abstract interface; the `ssh` crate implements it for `SftpSession`.
//!
//! The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.

use std::path::PathBuf;
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

// ── SftpBackend trait ────────────────────────────────────────

/// Abstract SFTP backend — implemented by the `ssh` crate.
///
/// The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.
/// Methods are sync — they bridge to async inside the implementation.
pub trait SftpBackend: Send + Sync + 'static {
    /// Read a directory — returns the list of entries.
    fn read_dir(&self, path: PathBuf) -> Result<Vec<FileEntry>>;

    /// Get detailed metadata.
    fn stat(&self, path: PathBuf) -> Result<FileStat>;

    /// Rename a file/folder.
    fn rename(&self, from: PathBuf, to: PathBuf) -> Result<()>;

    /// Remove a file.
    fn remove(&self, path: PathBuf) -> Result<()>;

    /// Remove an empty directory.
    fn rmdir(&self, path: PathBuf) -> Result<()>;

    /// Create a directory.
    fn mkdir(&self, path: PathBuf) -> Result<()>;

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
