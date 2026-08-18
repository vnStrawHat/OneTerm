//! SFTP abstraction — trait + types shared by the UI and the backend.
//!
//! `core` is a leaf crate (does not depend on `ssh`). The `SftpBackend` trait defines
//! the abstract interface; the `ssh` crate implements it for `SftpSession`.
//!
//! The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use async_channel::Receiver;

use crate::{AppError, Result};

// ── Remote path ──────────────────────────────────────────────

/// A path on the remote SFTP server.
///
/// Remote paths are POSIX: always `/`-separated, regardless of the host OS. This
/// type never uses `std::path`, because `PathBuf::join` inserts `\` on Windows and
/// the server would treat it as part of a file name.
///
/// Invariants (enforced by [`RemotePath::new`]):
/// - every `\` is turned into `/`,
/// - runs of `/` are collapsed into one,
/// - a trailing `/` is dropped unless the path is the root `/`.
///
/// The empty path (also the `Default`) is allowed and means "unset" (a browser with no directory
/// loaded yet). Relative paths such as `.` are allowed; the backend resolves
/// them against the server's default directory.
#[derive(Clone, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct RemotePath(String);

impl<'de> serde::Deserialize<'de> for RemotePath {
    /// Deserialize as a plain string, re-normalising so persisted or hand-edited
    /// values uphold the same invariants as constructed ones.
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

impl RemotePath {
    /// Build a normalised remote path from any string-like input.
    pub fn new(path: impl Into<String>) -> Self {
        let raw: String = path.into();
        let mut normalised = String::with_capacity(raw.len());
        let mut previous_was_separator = false;
        for character in raw.chars() {
            let character = if character == '\\' { '/' } else { character };
            if character == '/' {
                if previous_was_separator {
                    continue;
                }
                previous_was_separator = true;
            } else {
                previous_was_separator = false;
            }
            normalised.push(character);
        }
        if normalised.len() > 1 && normalised.ends_with('/') {
            normalised.pop();
        }
        Self(normalised)
    }

    /// The server root `/`.
    pub fn root() -> Self {
        Self("/".to_string())
    }

    /// The path as a `/`-separated string, ready for the SFTP wire.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for the "unset" path.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `true` when the path starts at the server root.
    pub fn is_absolute(&self) -> bool {
        self.0.starts_with('/')
    }

    /// `true` for the server root `/`.
    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    /// Append one or more components. An absolute `name` replaces `self`.
    pub fn join(&self, name: &str) -> Self {
        let name = name.replace('\\', "/");
        if name.starts_with('/') || self.0.is_empty() {
            return Self::new(name);
        }
        Self::new(format!("{}/{}", self.0, name))
    }

    /// The containing directory. `None` for the root, the empty path, and a
    /// bare relative name.
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() || self.0.is_empty() {
            return None;
        }
        let index = self.0.rfind('/')?;
        Some(if index == 0 {
            Self::root()
        } else {
            Self(self.0[..index].to_string())
        })
    }

    /// The last path component. `None` for the root, the empty path, `.` and `..`.
    pub fn file_name(&self) -> Option<&str> {
        let name = self.0.rsplit('/').next()?;
        (!name.is_empty() && name != "." && name != "..").then_some(name)
    }
}

impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RemotePath({:?})", self.0)
    }
}

impl From<&str> for RemotePath {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl From<String> for RemotePath {
    fn from(path: String) -> Self {
        Self::new(path)
    }
}

// ── File entry for UI rendering ──────────────────────────────

/// Metadata of one remote file or folder — a directory-listing row and the
/// result of `stat` alike (the properties dialog shows the same fields).
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: RemotePath,
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
        // A `u64` counter cannot realistically wrap within a single process run,
        // so a plain fetch-add stays unique without a fallible overflow check.
        // This mirrors the `TEMP_SEQUENCE` counter in `persistence`.
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        Self(id)
    }
}

// ── Transfers ────────────────────────────────────────────────

/// Progress notification for one running upload/download.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferEvent {
    /// Fraction of the transfer completed, in `0.0..=1.0`.
    Progress(f64),
    /// The transfer stopped because it was cancelled. The result channel then
    /// yields `Err(AppError::Cancelled)`.
    Cancelled,
}

/// Channels through which one upload/download reports back to the UI.
///
/// `events` closes when the backend stops reporting; `result` yields exactly one
/// value once the transfer has finished, failed, or been cancelled.
#[derive(Debug)]
pub struct TransferHandle {
    pub events: Receiver<TransferEvent>,
    pub result: Receiver<Result<()>>,
}

impl TransferHandle {
    /// A handle whose transfer already failed — used when the backend cannot
    /// even enqueue the request.
    pub fn failed(error: AppError) -> Self {
        let (_events_tx, events) = async_channel::bounded(1);
        let (result_tx, result) = async_channel::bounded(1);
        // The receiver was created just above and the channel has room for
        // one message, so this send cannot fail.
        let _ = result_tx.try_send(Err(error));
        Self { events, result }
    }
}

// ── SftpBackend trait ────────────────────────────────────────

/// Boxed SFTP operation future used by the object-safe backend contract.
pub type SftpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Abstract SFTP backend — implemented by the `ssh` crate.
///
/// The UI uses it via `dyn SftpBackend`, with no knowledge of `russh-sftp`.
/// Metadata and mutation operations are asynchronous and never block the UI thread.
/// Every remote location is a [`RemotePath`]; only the local side of a transfer
/// is a host `PathBuf`.
pub trait SftpBackend: Send + Sync + 'static {
    /// Stable identity for this backend instance, used to restore per-session UI state.
    fn session_id(&self) -> SftpSessionId;
    /// Read a directory — returns the list of entries.
    fn read_dir(&self, path: RemotePath) -> SftpFuture<'_, Vec<FileEntry>>;

    /// Get detailed metadata for one path (follows symlinks).
    fn stat(&self, path: RemotePath) -> SftpFuture<'_, FileEntry>;

    /// Resolve `path` — possibly relative to the server's default directory
    /// (e.g. `.`) or containing symlinks — to its absolute canonical form.
    fn realpath(&self, path: RemotePath) -> SftpFuture<'_, RemotePath>;

    /// Rename a file/folder.
    fn rename(&self, from: RemotePath, to: RemotePath) -> SftpFuture<'_, ()>;

    /// Remove a file.
    fn remove(&self, path: RemotePath) -> SftpFuture<'_, ()>;

    /// Remove a directory and everything below it.
    ///
    /// The traversal is bounded (depth and entry limits) and never follows
    /// symlinks: a symlinked root, or a symlink found below it, is unlinked
    /// rather than descended into. Passing a non-directory removes that entry.
    fn remove_dir_all(&self, path: RemotePath) -> SftpFuture<'_, ()>;

    /// Create a directory.
    fn mkdir(&self, path: RemotePath) -> SftpFuture<'_, ()>;

    /// Upload a local file or directory to `remote`.
    /// `transfer_id` — a unique ID created by the UI, used to cancel.
    /// The UI spawns a task to drive the returned handle — it does not block the UI thread.
    fn upload(&self, transfer_id: u64, local: PathBuf, remote: RemotePath) -> TransferHandle;

    /// Download a remote file or directory to `local`.
    /// Same as upload.
    fn download(&self, transfer_id: u64, remote: RemotePath, local: PathBuf) -> TransferHandle;

    /// Cancel a running transfer (upload/download).
    /// `transfer_id` must match the ID passed to `upload`/`download`.
    fn cancel_transfer(&self, transfer_id: u64);

    /// Close the SFTP session.
    fn close(&self);

    /// Is the SFTP session still alive?
    fn alive(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backslashes_become_forward_slashes() {
        assert_eq!(RemotePath::new("/home/u\\b.txt").as_str(), "/home/u/b.txt");
        assert_eq!(RemotePath::new("home\\u\\dir").as_str(), "home/u/dir");
    }

    #[test]
    fn duplicate_and_trailing_slashes_are_collapsed() {
        assert_eq!(RemotePath::new("//home//u///").as_str(), "/home/u");
        assert_eq!(RemotePath::new("/home/u/").as_str(), "/home/u");
        assert_eq!(RemotePath::new("///").as_str(), "/");
        assert_eq!(RemotePath::new("/").as_str(), "/");
    }

    #[test]
    fn root_and_empty_paths() {
        assert!(RemotePath::root().is_root());
        assert!(RemotePath::root().is_absolute());
        assert_eq!(RemotePath::root().as_str(), "/");
        assert!(RemotePath::new("").is_empty());
        assert!(!RemotePath::new(".").is_absolute());
    }

    #[test]
    fn join_uses_forward_slashes_on_every_host() {
        assert_eq!(
            RemotePath::new("/home/u").join("b.txt").as_str(),
            "/home/u/b.txt"
        );
        assert_eq!(RemotePath::root().join("etc").as_str(), "/etc");
        assert_eq!(
            RemotePath::new("/home/u").join("dir\\sub").as_str(),
            "/home/u/dir/sub"
        );
        assert_eq!(RemotePath::new("").join("rel").as_str(), "rel");
        assert_eq!(RemotePath::new("/home").join("/abs").as_str(), "/abs");
    }

    #[test]
    fn parent_walks_up_to_root_then_stops() {
        assert_eq!(
            RemotePath::new("/home/u/b.txt").parent(),
            Some(RemotePath::new("/home/u"))
        );
        assert_eq!(RemotePath::new("/home").parent(), Some(RemotePath::root()));
        assert_eq!(RemotePath::root().parent(), None);
        assert_eq!(RemotePath::new("").parent(), None);
        assert_eq!(RemotePath::new("name").parent(), None);
        assert_eq!(RemotePath::new("a/b").parent(), Some(RemotePath::new("a")));
    }

    #[test]
    fn file_name_is_the_last_component() {
        assert_eq!(RemotePath::new("/home/u/b.txt").file_name(), Some("b.txt"));
        assert_eq!(RemotePath::new("/home/u/").file_name(), Some("u"));
        assert_eq!(RemotePath::new("name").file_name(), Some("name"));
        assert_eq!(RemotePath::root().file_name(), None);
        assert_eq!(RemotePath::new("").file_name(), None);
        assert_eq!(RemotePath::new(".").file_name(), None);
    }

    #[test]
    fn display_and_serde_use_the_plain_string() {
        let path = RemotePath::new("/home\\u");
        assert_eq!(path.to_string(), "/home/u");
        assert_eq!(serde_json::to_string(&path).unwrap(), "\"/home/u\"");
        let restored: RemotePath = serde_json::from_str("\"/var\\\\log\"").unwrap();
        assert_eq!(restored, RemotePath::new("/var/log"));
    }

    #[test]
    fn failed_transfer_handle_reports_the_error_once() {
        let handle = TransferHandle::failed(AppError::msg("enqueue"));
        assert!(handle.events.try_recv().is_err());
        let error = handle.result.try_recv().unwrap().unwrap_err();
        assert_eq!(error.to_string(), "enqueue");
    }
}
