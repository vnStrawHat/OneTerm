//! SFTP tokio task — handles `SftpCmd` from the UI, calls the `russh_sftp` API.
//!
//! Runs alongside `ssh_main_task` on the same tokio runtime.
//! The two channels (shell + sftp) share one TCP connection, multiplexed by russh.
//!
//! Upload/download are spawned as separate tokio tasks — the main loop stays
//! responsive to receive `SftpCmd::Cancel` and signal the `CancellationToken`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use async_channel::{Receiver, Sender};
use russh_sftp::client::SftpSession as SftpChannel;
use russh_sftp::protocol::FileAttributes;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[path = "sftp_transfer.rs"]
mod sftp_transfer;
use sftp_transfer::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES, sftp_download, sftp_upload};

use oneterm_core::{AppError, FileEntry, FileStat, Result};

use crate::sftp::{SftpCmd, SftpEvent};

// ── UID/GID lookup ────────────────────────────────────────────

/// Looks up uid → username and gid → groupname, parsed from /etc/passwd +
/// /etc/group. Cached once when the SFTP task starts, used by every
/// `attrs_to_entry`.
#[derive(Default)]
struct UidGidLookup {
    uid_to_name: HashMap<u32, String>,
    gid_to_name: HashMap<u32, String>,
}

impl UidGidLookup {
    /// Resolve uid → username. None if not in the map.
    fn uid_name(&self, uid: Option<u32>) -> Option<String> {
        uid.and_then(|u| self.uid_to_name.get(&u).cloned())
    }

    /// Resolve gid → groupname. None if not in the map.
    fn gid_name(&self, gid: Option<u32>) -> Option<String> {
        gid.and_then(|g| self.gid_to_name.get(&g).cloned())
    }
}

/// Read /etc/passwd + /etc/group over SFTP, parse uid→name and gid→name maps.
/// Best-effort: if unreadable → empty map (numbers shown instead).
async fn load_uid_gid_lookup(sftp: &SftpChannel) -> UidGidLookup {
    let mut lookup = UidGidLookup::default();

    // /etc/passwd: `root:x:0:0:root:/root:/bin/bash`
    //             field 0 = name, field 2 = uid, field 3 = gid
    match sftp.read("/etc/passwd").await {
        Ok(data) => {
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 4 {
                    if let (Ok(uid), Ok(gid)) = (fields[2].parse::<u32>(), fields[3].parse::<u32>())
                    {
                        lookup.uid_to_name.insert(uid, fields[0].to_string());
                        // passwd also has gid → can be used for group lookup.
                        lookup
                            .gid_to_name
                            .entry(gid)
                            .or_insert_with(|| fields[0].to_string());
                    }
                }
            }
            log::debug!(
                "sftp_task: /etc/passwd loaded — {} uids",
                lookup.uid_to_name.len()
            );
        }
        Err(e) => {
            log::debug!("sftp_task: /etc/passwd not readable: {e} — uid/gid shown as numbers")
        }
    }

    // /etc/group: `root:x:0:`
    //             field 0 = name, field 2 = gid
    match sftp.read("/etc/group").await {
        Ok(data) => {
            let text = String::from_utf8_lossy(&data);
            for line in text.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() >= 3 {
                    if let Ok(gid) = fields[2].parse::<u32>() {
                        lookup.gid_to_name.insert(gid, fields[0].to_string());
                    }
                }
            }
            log::debug!(
                "sftp_task: /etc/group loaded — {} gids",
                lookup.gid_to_name.len()
            );
        }
        Err(e) => log::debug!("sftp_task: /etc/group not readable: {e}"),
    }

    lookup
}

type ActiveTransfers = Arc<std::sync::Mutex<HashMap<u64, CancellationToken>>>;

struct ActiveTransferGuard {
    transfers: ActiveTransfers,
    transfer_id: u64,
}

impl ActiveTransferGuard {
    fn new(transfers: ActiveTransfers, transfer_id: u64) -> Self {
        Self {
            transfers,
            transfer_id,
        }
    }
}

impl Drop for ActiveTransferGuard {
    fn drop(&mut self) {
        self.transfers
            .lock()
            .expect("SFTP cancellation map is not poisoned")
            .remove(&self.transfer_id);
    }
}

/// Tokio task that handles SFTP commands.
///
/// Runs alongside `ssh_main_task` on the same tokio runtime.
/// Receives `SftpCmd` via `cmd_rx`, sends `SftpEvent` via `event_tx`.
///
/// Upload/download are spawned as separate tokio tasks — the main loop stays
/// responsive to receive `SftpCmd::Cancel` and signal the `CancellationToken`.
pub(crate) async fn sftp_task(
    sftp: SftpChannel,
    cmd_rx: Receiver<SftpCmd>,
    event_tx: Sender<SftpEvent>,
    alive: std::sync::Arc<std::sync::Mutex<bool>>,
) {
    log::info!("sftp_task: started");

    // Load uid→name and gid→name maps from /etc/passwd + /etc/group.
    // Best-effort: if unreadable → empty map, numbers shown.
    let lookup = load_uid_gid_lookup(&sftp).await;
    log::info!(
        "sftp_task: uid/gid lookup loaded ({} uids, {} gids)",
        lookup.uid_to_name.len(),
        lookup.gid_to_name.len()
    );

    // Wrap SftpChannel in Arc — cloned for each spawned transfer task.
    let sftp = Arc::new(sftp);

    let _ = event_tx.try_send(SftpEvent::Ready);

    // Cancel tokens for running transfers — key = transfer_id.
    let cancels: ActiveTransfers = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut background_tasks = JoinSet::new();

    loop {
        let command = tokio::select! {
            command = cmd_rx.recv() => Some(command),
            completed = background_tasks.join_next(), if !background_tasks.is_empty() => {
                if let Some(Err(error)) = completed {
                    log::error!("sftp_task: background task failed: {error}");
                }
                None
            }
        };
        let Some(command) = command else {
            continue;
        };
        match command {
            Ok(SftpCmd::ReadDir { path, reply }) => {
                log::debug!("sftp_task: ReadDir path=\"{}\"", path.display());
                let result = sftp_read_dir(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Stat { path, reply }) => {
                log::debug!("sftp_task: Stat path=\"{}\"", path.display());
                let result = sftp_stat(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rename { from, to, reply }) => {
                log::debug!(
                    "sftp_task: Rename from=\"{}\" to=\"{}\"",
                    from.display(),
                    to.display()
                );
                let result = sftp
                    .rename(from.to_string_lossy(), to.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Remove { path, reply }) => {
                log::debug!("sftp_task: Remove path=\"{}\"", path.display());
                let result = sftp
                    .remove_file(path.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rmdir { path, reply }) => {
                log::debug!("sftp_task: Rmdir path=\"{}\"", path.display());
                let sftp = Arc::clone(&sftp);
                background_tasks.spawn(async move {
                    let result = sftp_remove_recursive(&sftp, &path).await;
                    let _ = reply.send(result);
                });
            }
            Ok(SftpCmd::Mkdir { path, reply }) => {
                log::debug!("sftp_task: Mkdir path=\"{}\"", path.display());
                let result = sftp
                    .create_dir(path.to_string_lossy())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Upload {
                transfer_id,
                local,
                remote,
                progress,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Upload #{transfer_id} local=\"{}\" remote=\"{}\"",
                    local.display(),
                    remote.display()
                );
                let cancel = CancellationToken::new();
                let mut active = cancels
                    .lock()
                    .expect("SFTP cancellation map is not poisoned");
                if active.contains_key(&transfer_id) {
                    let _ = reply.try_send(Err(AppError::msg(format!(
                        "duplicate active transfer id: {transfer_id}"
                    ))));
                    continue;
                }
                active.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                let cancels = Arc::clone(&cancels);
                background_tasks.spawn(async move {
                    let _cleanup = ActiveTransferGuard::new(cancels, transfer_id);
                    let result = sftp_upload(&sftp, &local, &remote, &progress, &cancel).await;
                    log::info!(
                        "sftp_task: Upload #{transfer_id} finished: {}",
                        if result.is_ok() { "OK" } else { "error" }
                    );
                    let _ = reply.try_send(result);
                });
            }
            Ok(SftpCmd::Download {
                transfer_id,
                remote,
                local,
                progress,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Download #{transfer_id} remote=\"{}\" local=\"{}\"",
                    remote.display(),
                    local.display()
                );
                let cancel = CancellationToken::new();
                let mut active = cancels
                    .lock()
                    .expect("SFTP cancellation map is not poisoned");
                if active.contains_key(&transfer_id) {
                    let _ = reply.try_send(Err(AppError::msg(format!(
                        "duplicate active transfer id: {transfer_id}"
                    ))));
                    continue;
                }
                active.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                let cancels = Arc::clone(&cancels);
                background_tasks.spawn(async move {
                    let _cleanup = ActiveTransferGuard::new(cancels, transfer_id);
                    let result = sftp_download(&sftp, &remote, &local, &progress, &cancel).await;
                    log::info!(
                        "sftp_task: Download #{transfer_id} finished: {}",
                        if result.is_ok() { "OK" } else { "error" }
                    );
                    let _ = reply.try_send(result);
                });
            }
            Ok(SftpCmd::Cancel { transfer_id }) => {
                log::info!("sftp_task: Cancel transfer #{transfer_id}");
                let cancel = cancels
                    .lock()
                    .expect("SFTP cancellation map is not poisoned")
                    .get(&transfer_id)
                    .cloned();
                if let Some(cancel) = cancel {
                    cancel.cancel();
                    log::info!("sftp_task: Cancel #{transfer_id} — token signalled");
                } else {
                    log::warn!("sftp_task: Cancel #{transfer_id} — not found (already finished?)");
                }
            }
            Ok(SftpCmd::Close) => {
                log::info!("sftp_task: close requested");
                for cancellation in cancels
                    .lock()
                    .expect("SFTP cancellation map is not poisoned")
                    .values()
                {
                    cancellation.cancel();
                }
                break;
            }
            Err(_) => {
                log::info!("sftp_task: cmd_rx closed — session dropped");
                for cancellation in cancels
                    .lock()
                    .expect("SFTP cancellation map is not poisoned")
                    .values()
                {
                    cancellation.cancel();
                }
                break;
            }
        }
    }

    const BACKGROUND_TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
    let drain_tasks = async {
        while let Some(result) = background_tasks.join_next().await {
            if let Err(error) = result {
                log::error!("sftp_task: background task failed during shutdown: {error}");
            }
        }
    };
    if tokio::time::timeout(BACKGROUND_TASK_SHUTDOWN_TIMEOUT, drain_tasks)
        .await
        .is_err()
    {
        log::warn!("sftp_task: background-task shutdown timed out; aborting remaining tasks");
        background_tasks.abort_all();
        while let Some(result) = background_tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                log::error!("sftp_task: aborted background task failed: {error}");
            }
        }
    }
    cancels
        .lock()
        .expect("SFTP cancellation map is not poisoned")
        .clear();

    {
        let mut a = alive.lock().unwrap();
        *a = false;
    }
    let _ = event_tx.try_send(SftpEvent::Closed);
    log::info!("sftp_task: exiting");
}

// ── Helpers ──────────────────────────────────────────────────

/// Convert a russh-sftp error to `AppError`.
pub(crate) fn map_sftp_err(e: russh_sftp::client::error::Error) -> AppError {
    AppError::msg(e.to_string())
}

/// Validate one remote directory entry before using it as a local path component.
///
/// Remote names are treated as untrusted input. They must remain one normal
/// component on every supported client platform; separators, prefixes, reserved
/// Windows names, and trailing dot/space forms are rejected.
pub(crate) fn validate_remote_entry_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(AppError::msg("unsafe remote filename"));
    }
    if name
        .chars()
        .any(|ch| ch == '/' || ch == '\\' || ch == ':' || ch == '\0')
    {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    if !matches!(
        Path::new(name).components().next(),
        Some(std::path::Component::Normal(component))
            if component == std::ffi::OsStr::new(name)
    ) {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }

    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    Ok(())
}

/// Join a validated remote name to a local root without allowing traversal.
pub(crate) fn safe_local_child(root: &Path, name: &str) -> Result<PathBuf> {
    validate_remote_entry_name(name)?;
    let child = root.join(name);
    if !child.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    Ok(child)
}

/// Reject existing symlinks and verify that a local destination remains below
/// the selected root before writing it.
async fn ensure_local_destination(root: &Path, candidate: &Path) -> Result<()> {
    if !candidate.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    let relative = candidate
        .strip_prefix(root)
        .map_err(|_| AppError::msg("local download path escaped destination"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(AppError::msg("unsafe local download component"));
        };
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::msg(format!(
                    "refusing to traverse local symlink: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::msg(format!(
                    "inspect local download path {}: {error}",
                    current.display()
                )));
            }
        }
    }

    let parent = candidate.parent().unwrap_or(root);
    let canonical_parent = tokio::fs::canonicalize(parent)
        .await
        .map_err(|e| AppError::msg(format!("canonicalize download parent: {e}")))?;
    if !canonical_parent.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    Ok(())
}

/// Create missing destination directories one component at a time while
/// rejecting pre-existing symlinks and non-directory components.
pub(crate) async fn create_safe_parent_dirs(root: &Path, candidate: &Path) -> Result<()> {
    let parent = candidate.parent().unwrap_or(root);
    let relative = parent
        .strip_prefix(root)
        .map_err(|_| AppError::msg("local download path escaped destination"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(AppError::msg("unsafe local download component"));
        };
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::msg(format!(
                    "refusing to traverse local symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(AppError::msg(format!(
                    "local download parent is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&current)
                    .await
                    .map_err(|e| AppError::msg(format!("create local directory: {e}")))?;
            }
            Err(error) => {
                return Err(AppError::msg(format!(
                    "inspect local download path {}: {error}",
                    current.display()
                )));
            }
        }
        let canonical = tokio::fs::canonicalize(&current)
            .await
            .map_err(|e| AppError::msg(format!("canonicalize download directory: {e}")))?;
        if !canonical.starts_with(root) {
            return Err(AppError::msg("local download path escaped destination"));
        }
    }
    ensure_local_destination(root, candidate).await
}

/// Convert `FileAttributes` (russh-sftp) to a `FileEntry`.
///
/// IMPORTANT: SFTP paths always use `/` (Unix style), even when the client runs
/// on Windows. `PathBuf::join` on Windows uses `\` → the SFTP server won't
/// understand it. So use string concatenation with `/` instead of `Path::join`.
fn attrs_to_entry(
    name: String,
    parent: &str,
    attrs: &FileAttributes,
    lookup: &UidGidLookup,
) -> FileEntry {
    // Join parent + name with `/` — ensures a Unix-style path for SFTP.
    let path = if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    };
    let uid = attrs.uid;
    let gid = attrs.gid;
    FileEntry {
        name,
        path: PathBuf::from(path),
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        modified: attrs
            .mtime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        accessed: attrs
            .atime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        permissions: attrs.permissions.unwrap_or(0),
        uid,
        gid,
        owner: lookup.uid_name(uid),
        group: lookup.gid_name(gid),
    }
}

/// Read a directory — returns the sorted list of entries (folders first, then
/// files by name).
///
/// If `path` is relative (e.g. `"."`), use `canonicalize` to resolve it to an
/// absolute path first — some SFTP servers don't understand relative paths.
async fn sftp_read_dir(
    sftp: &SftpChannel,
    path: &Path,
    lookup: &UidGidLookup,
) -> Result<Vec<FileEntry>> {
    // to_string_lossy() may return `\` on Windows → convert to `/`.
    let path_str = path.to_string_lossy().replace('\\', "/");
    log::debug!("sftp_read_dir: path=\"{path_str}\"");

    // Resolve relative path → absolute path via SFTP realpath.
    // Use starts_with('/') instead of Path::is_absolute() because Windows does
    // not treat `/root` as absolute (it needs a drive letter).
    let abs_path = if path_str.starts_with('/') {
        path_str
    } else {
        match sftp.canonicalize(&path_str).await {
            Ok(resolved) => {
                log::debug!("sftp_read_dir: canonicalize(\"{path_str}\") → \"{resolved}\"");
                resolved
            }
            Err(e) => {
                log::warn!(
                    "sftp_read_dir: canonicalize(\"{path_str}\") failed: {e} — trying original path"
                );
                path_str
            }
        }
    };

    // abs_path is already a string with `/` separators (from canonicalize or input).
    // Do NOT use Path::new — PathBuf on Windows would convert `/` → `\`.

    let read_dir = sftp.read_dir(&abs_path).await.map_err(|e| {
        log::error!("sftp_read_dir: read_dir(\"{abs_path}\") failed: {e}");
        map_sftp_err(e)
    })?;

    let mut entries: Vec<FileEntry> = read_dir
        .map(|entry| {
            let name = entry.file_name();
            let attrs = entry.metadata();
            attrs_to_entry(name, &abs_path, &attrs, lookup)
        })
        .collect();

    // Drop `.` and `..` entries (returned by some SFTP servers).
    entries.retain(|e| e.name != "." && e.name != "..");

    log::debug!(
        "sftp_read_dir: got {} entries for \"{abs_path}\"",
        entries.len()
    );

    // Sort: folders first, then files by name (case-insensitive).
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Get detailed metadata.
async fn sftp_stat(sftp: &SftpChannel, path: &Path, lookup: &UidGidLookup) -> Result<FileStat> {
    // Sanitize backslashes → forward slashes for the SFTP server.
    let path_str = path.to_string_lossy().replace('\\', "/");
    let attrs = sftp.metadata(&path_str).await.map_err(map_sftp_err)?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(FileStat {
        name,
        path: path.to_path_buf(),
        is_dir: attrs.is_dir(),
        is_symlink: attrs.is_symlink(),
        size: attrs.size.unwrap_or(0),
        modified: attrs
            .mtime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        accessed: attrs
            .atime
            .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
        permissions: attrs.permissions.unwrap_or(0),
        uid: attrs.uid,
        gid: attrs.gid,
        owner: lookup.uid_name(attrs.uid),
        group: lookup.gid_name(attrs.gid),
    })
}

/// Remove a file/directory recursively — if a directory, read its contents →
/// remove each entry → remove the dir.
/// Used for `SftpCmd::Rmdir` — supports removing non-empty directories.
async fn sftp_remove_recursive(sftp: &SftpChannel, path: &Path) -> Result<()> {
    let root = path.to_string_lossy().replace('\\', "/");
    let mut pending = vec![(root, false, 0usize)];
    let mut visited = 0usize;

    while let Some((current, expanded, depth)) = pending.pop() {
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg(
                "remote deletion exceeded traversal depth limit",
            ));
        }
        if expanded {
            sftp.remove_dir(&current).await.map_err(map_sftp_err)?;
            continue;
        }

        let read_dir = match sftp.read_dir(&current).await {
            Ok(entries) => entries,
            Err(_) => {
                sftp.remove_file(&current).await.map_err(map_sftp_err)?;
                continue;
            }
        };
        pending.push((current.clone(), true, depth));

        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            validate_remote_entry_name(&name)?;
            visited += 1;
            if visited > MAX_TRAVERSAL_ENTRIES {
                return Err(AppError::msg(
                    "remote deletion exceeded traversal entry limit",
                ));
            }
            let child = format!("{}/{}", current.trim_end_matches('/'), name);
            let metadata = entry.metadata();
            if metadata.is_dir() && !metadata.is_symlink() {
                pending.push((child, false, depth + 1));
            } else {
                sftp.remove_file(&child).await.map_err(map_sftp_err)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod security_tests {
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::sftp_transfer::{LocalUploadEntry, stream_local_upload_entries};
    #[cfg(unix)]
    use super::sftp_transfer::{finalize_local_file, temporary_local_sibling};
    use super::*;

    #[cfg(unix)]
    fn temporary_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "oneterm-sftp-security-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_remote_names_that_can_escape_or_alias() {
        for name in [
            "",
            ".",
            "..",
            "../outside",
            "dir/file",
            "dir\\file",
            "/absolute",
            "C:outside",
            "name\0tail",
            "CON",
            "nul.txt",
            "COM1.log",
            "trailing.",
            "trailing ",
        ] {
            assert!(
                validate_remote_entry_name(name).is_err(),
                "{name:?} must be rejected"
            );
        }
    }

    #[test]
    fn accepts_one_safe_component_and_keeps_it_below_root() {
        let root = Path::new("download-root");
        for name in ["file.txt", "folder", "héllo 世界.txt", "..safe"] {
            let child = safe_local_child(root, name).unwrap();
            assert_eq!(child, root.join(name));
            assert!(child.starts_with(root));
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_finalization_replaces_only_after_complete_write() {
        let root = temporary_dir();
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("result.txt");
        let temporary = temporary_local_sibling(&target, "part").unwrap();
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&temporary, b"complete").unwrap();

        finalize_local_file(&temporary, &target).await.unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"complete");
        assert!(!temporary.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_preexisting_symlink_below_download_root() {
        use std::os::unix::fs::symlink;

        let root = temporary_dir();
        let outside = temporary_dir();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        symlink(&outside, root.join("linked")).unwrap();
        let candidate = root.join("linked").join("file.txt");

        let error = create_safe_parent_dirs(&root, &candidate)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("symlink"));

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn active_transfer_guard_removes_token_on_drop() {
        let transfers: ActiveTransfers = Arc::new(std::sync::Mutex::new(HashMap::new()));
        transfers
            .lock()
            .unwrap()
            .insert(7, CancellationToken::new());
        {
            let _guard = ActiveTransferGuard::new(Arc::clone(&transfers), 7);
        }
        assert!(transfers.lock().unwrap().is_empty());
    }

    #[test]
    fn local_traversal_stops_before_filesystem_access_when_cancelled() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (entries, _receiver) = async_channel::bounded(1);
        let error = stream_local_upload_entries(
            PathBuf::from("missing"),
            PathBuf::from("/remote"),
            cancel,
            &entries,
        )
        .unwrap_err();
        assert!(matches!(error, AppError::Cancelled));
    }

    #[test]
    fn local_upload_discovery_cancels_while_channel_is_full() {
        let cancel = CancellationToken::new();
        let (entries, _receiver) = async_channel::bounded(1);
        entries
            .try_send(LocalUploadEntry::Directory(PathBuf::from("/occupied")))
            .unwrap();
        let traversal_cancel = cancel.clone();
        let traversal = std::thread::spawn(move || {
            stream_local_upload_entries(
                PathBuf::from("missing"),
                PathBuf::from("/remote"),
                traversal_cancel,
                &entries,
            )
        });

        std::thread::sleep(std::time::Duration::from_millis(10));
        cancel.cancel();
        assert!(matches!(
            traversal.join().unwrap().unwrap_err(),
            AppError::Cancelled
        ));
    }

    #[cfg(unix)]
    #[test]
    fn local_upload_discovery_streams_directories_and_files() {
        let root = temporary_dir();
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("first.txt"), b"first").unwrap();
        std::fs::write(root.join("nested").join("second.txt"), b"second").unwrap();
        let (entries, receiver) = async_channel::bounded(8);

        stream_local_upload_entries(
            root.clone(),
            PathBuf::from("/remote"),
            CancellationToken::new(),
            &entries,
        )
        .unwrap();
        drop(entries);
        let discovered: Vec<_> = receiver.try_iter().collect();

        assert_eq!(
            discovered
                .iter()
                .filter(|entry| matches!(entry, LocalUploadEntry::Directory(_)))
                .count(),
            2
        );
        assert_eq!(
            discovered
                .iter()
                .filter(|entry| matches!(entry, LocalUploadEntry::File { .. }))
                .count(),
            2
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
