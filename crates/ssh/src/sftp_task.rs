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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

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
    let mut cancels: HashMap<u64, CancellationToken> = HashMap::new();

    loop {
        match cmd_rx.recv().await {
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
                tokio::spawn(async move {
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
                cancels.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                tokio::spawn(async move {
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
                cancels.insert(transfer_id, cancel.clone());
                let sftp = Arc::clone(&sftp);
                tokio::spawn(async move {
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
                if let Some(cancel) = cancels.get(&transfer_id) {
                    cancel.cancel();
                    log::info!("sftp_task: Cancel #{transfer_id} — token signalled");
                } else {
                    log::warn!("sftp_task: Cancel #{transfer_id} — not found (already finished?)");
                }
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
        // Cleanup: remove cancel tokens for finished transfers.
        // Tokens are inserted when an upload/download begins. The spawned task
        // cannot remove them from the map → they stay. The map is small (only
        // running transfers), so this is negligible.
    }

    {
        let mut a = alive.lock().unwrap();
        *a = false;
    }
    let _ = event_tx.try_send(SftpEvent::Closed);
    log::info!("sftp_task: exiting");
}

// ── Helpers ──────────────────────────────────────────────────

/// Convert a russh-sftp error to `AppError`.
fn map_sftp_err(e: russh_sftp::client::error::Error) -> AppError {
    AppError::msg(e.to_string())
}

/// Validate one remote directory entry before using it as a local path component.
///
/// Remote names are treated as untrusted input. They must remain one normal
/// component on every supported client platform; separators, prefixes, reserved
/// Windows names, and trailing dot/space forms are rejected.
fn validate_remote_entry_name(name: &str) -> Result<()> {
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
fn safe_local_child(root: &Path, name: &str) -> Result<PathBuf> {
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
async fn create_safe_parent_dirs(root: &Path, candidate: &Path) -> Result<()> {
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
    let path_str = path.to_string_lossy().replace('\\', "/");

    // Read the directory — if read_dir fails, the path may be a file → try remove_file.
    let read_dir = match sftp.read_dir(&path_str).await {
        Ok(rd) => rd,
        Err(_) => {
            // Path may be a file → try remove_file.
            log::debug!("sftp_remove_recursive: \"{path_str}\" not a dir, trying remove_file");
            return sftp.remove_file(&path_str).await.map_err(map_sftp_err);
        }
    };

    let entries: Vec<(String, bool)> = read_dir
        .filter_map(|e| {
            let name = e.file_name();
            if name == "." || name == ".." {
                return None;
            }
            let is_dir = e.metadata().is_dir();
            Some((name, is_dir))
        })
        .collect();

    for (name, is_dir) in entries {
        // Use string concat with '/' instead of Path::join (avoids '\' on Windows).
        let child_path = format!("{path_str}/{name}");
        if is_dir {
            Box::pin(sftp_remove_recursive(sftp, &PathBuf::from(&child_path))).await?;
        } else {
            log::debug!("sftp_remove_recursive: remove_file \"{child_path}\"");
            sftp.remove_file(&child_path).await.map_err(map_sftp_err)?;
        }
    }

    // Directory is now empty → remove_dir.
    log::debug!("sftp_remove_recursive: remove_dir \"{path_str}\"");
    sftp.remove_dir(&path_str).await.map_err(map_sftp_err)
}

/// Upload a local file or directory → remote with progress reporting.
///
/// - File: read contents → write 32KB chunks → report progress 0.0–1.0.
/// - Directory: walk recursively → create remote dirs → upload each file,
///   progress = cumulative bytes / total bytes.
///
/// Checks `cancel.is_cancelled()` after each chunk write.
/// If cancelled → returns `Err("cancelled")`.
async fn sftp_upload(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let metadata = tokio::fs::metadata(local)
        .await
        .map_err(|e| AppError::msg(format!("stat local: {e}")))?;

    if metadata.is_dir() {
        sftp_upload_dir(sftp, local, remote, progress, cancel).await
    } else {
        sftp_upload_file(sftp, local, remote, progress, cancel).await
    }
}

/// Upload a single file — 32KB chunks, progress 0.0–1.0.
async fn sftp_upload_file(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let local_data = tokio::fs::read(local)
        .await
        .map_err(|e| AppError::msg(format!("read local: {e}")))?;
    let total = local_data.len() as u64;

    // Use `create` — open the file with WRITE|CREATE|TRUNCATE.
    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let mut remote_file = sftp.create(&remote_str).await.map_err(map_sftp_err)?;

    const CHUNK: usize = 32 * 1024;
    let mut written: u64 = 0;
    for chunk in local_data.chunks(CHUNK) {
        // Check cancel before writing — stop immediately if cancelled.
        if cancel.is_cancelled() {
            log::info!("sftp_upload_file: cancelled at {written}/{total} bytes");
            let _ = progress.try_send(-1.0); // -1 = cancelled signal
            return Err(AppError::msg("cancelled"));
        }
        remote_file
            .write_all(chunk)
            .await
            .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
        written += chunk.len() as u64;
        let pct = if total > 0 {
            written as f64 / total as f64
        } else {
            1.0
        };
        let _ = progress.try_send(pct);
    }

    remote_file
        .flush()
        .await
        .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
    let _ = progress.try_send(1.0);

    Ok(())
}

/// Upload a directory — walk recursively, create remote dirs, upload each file.
///
/// Progress = cumulative bytes uploaded / total bytes across all files.
async fn sftp_upload_dir(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    /// Collect all files (local_path, remote_path, size) in the directory.
    fn collect_files(
        local: &Path,
        remote: &Path,
        files: &mut Vec<(PathBuf, PathBuf, u64)>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(local)? {
            let entry = entry?;
            let path = entry.path();
            let remote_child = remote.join(entry.file_name());
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                collect_files(&path, &remote_child, files)?;
            } else {
                files.push((path, remote_child, metadata.len()));
            }
        }
        Ok(())
    }

    // 1. Collect the file list + compute total size.
    let mut files: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    collect_files(local, remote, &mut files)
        .map_err(|e| AppError::msg(format!("walk local dir: {e}")))?;
    let total_bytes: u64 = files.iter().map(|(_, _, s)| *s).sum();
    log::info!(
        "sftp_upload_dir: \"{}\" → \"{}\" — {} files, {} bytes",
        local.display(),
        remote.display(),
        files.len(),
        total_bytes
    );

    // 2. Collect all remote dirs to create (DFS, parents first).
    fn collect_dirs(local: &Path, remote: &Path, dirs: &mut Vec<PathBuf>) {
        dirs.push(remote.to_path_buf());
        if let Ok(entries) = std::fs::read_dir(local) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let remote_child = remote.join(entry.file_name());
                    collect_dirs(&path, &remote_child, dirs);
                }
            }
        }
    }

    let mut remote_dirs: Vec<PathBuf> = Vec::new();
    collect_dirs(local, remote, &mut remote_dirs);
    for dir in &remote_dirs {
        let dir_str = dir.to_string_lossy().replace('\\', "/");
        // Create the dir (ignore error if it already exists).
        if let Err(e) = sftp.create_dir(&dir_str).await {
            log::debug!("sftp_upload_dir: create_dir \"{dir_str}\" → {e} (may already exist)");
        }
    }

    // 3. Upload each file, tracking cumulative progress.
    let mut bytes_done: u64 = 0;
    for (local_path, remote_path, file_size) in &files {
        if cancel.is_cancelled() {
            log::info!("sftp_upload_dir: cancelled at {bytes_done}/{total_bytes} bytes");
            let _ = progress.try_send(-1.0);
            return Err(AppError::msg("cancelled"));
        }

        log::debug!(
            "sftp_upload_dir: uploading \"{}\" → \"{}\" ({file_size} bytes)",
            local_path.display(),
            remote_path.display()
        );

        // Upload the file inline — report progress based on cumulative bytes.
        let local_data = tokio::fs::read(local_path)
            .await
            .map_err(|e| AppError::msg(format!("read local: {e}")))?;

        let remote_str = remote_path.to_string_lossy().replace('\\', "/");
        let mut remote_file = sftp.create(&remote_str).await.map_err(map_sftp_err)?;

        const CHUNK: usize = 32 * 1024;
        for chunk in local_data.chunks(CHUNK) {
            if cancel.is_cancelled() {
                log::info!("sftp_upload_dir: cancelled mid-file at {bytes_done}/{total_bytes}");
                let _ = progress.try_send(-1.0);
                return Err(AppError::msg("cancelled"));
            }
            remote_file
                .write_all(chunk)
                .await
                .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
            bytes_done += chunk.len() as u64;
            let pct = if total_bytes > 0 {
                bytes_done as f64 / total_bytes as f64
            } else {
                1.0
            };
            let _ = progress.try_send(pct);
        }

        remote_file
            .flush()
            .await
            .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
    }

    let _ = progress.try_send(1.0);
    Ok(())
}

/// Download a remote file or directory → local with progress reporting.
///
/// - File: open the remote file → read 32KB chunks → write the local file,
///   progress 0.0–1.0.
/// - Directory: walk the remote tree recursively → create local dirs → download
///   each file, progress = cumulative bytes / total bytes.
///
/// Checks `cancel.is_cancelled()` after each chunk read.
/// If cancelled → returns `Err("cancelled")`.
async fn sftp_download(
    sftp: &SftpChannel,
    remote: &Path,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let attrs = sftp.metadata(&remote_str).await.map_err(map_sftp_err)?;
    if attrs.is_symlink() {
        return Err(AppError::msg("refusing to download a remote symlink"));
    }

    if attrs.is_dir() {
        sftp_download_dir(sftp, &remote_str, local, progress, cancel).await
    } else {
        sftp_download_file(sftp, &remote_str, local, progress, cancel).await
    }
}

/// Download a single file — 32KB chunks, progress 0.0–1.0.
async fn sftp_download_file(
    sftp: &SftpChannel,
    remote_str: &str,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    // Get the size to compute progress.
    let attrs = sftp.metadata(remote_str).await.map_err(map_sftp_err)?;
    if attrs.is_symlink() {
        return Err(AppError::msg("refusing to download a remote symlink"));
    }
    if let Ok(metadata) = tokio::fs::symlink_metadata(local).await {
        if metadata.file_type().is_symlink() {
            return Err(AppError::msg("refusing to overwrite a local symlink"));
        }
    }
    let total = attrs.size.unwrap_or(0);

    let mut remote_file = sftp.open(remote_str).await.map_err(map_sftp_err)?;

    let mut local_file = tokio::fs::File::create(local)
        .await
        .map_err(|e| AppError::msg(format!("create local: {e}")))?;

    const CHUNK: usize = 32 * 1024;
    let mut buf = vec![0u8; CHUNK];
    let mut read: u64 = 0;
    loop {
        // Check cancel before reading — stop immediately if cancelled.
        if cancel.is_cancelled() {
            log::info!("sftp_download_file: cancelled at {read}/{total} bytes");
            let _ = progress.try_send(-1.0); // -1 = cancelled signal
            return Err(AppError::msg("cancelled"));
        }
        let n = remote_file
            .read(&mut buf)
            .await
            .map_err(|e| AppError::msg(format!("read remote: {e}")))?;
        if n == 0 {
            break;
        }
        local_file
            .write_all(&buf[..n])
            .await
            .map_err(|e| AppError::msg(format!("write local: {e}")))?;
        read += n as u64;
        let pct = if total > 0 {
            read as f64 / total as f64
        } else {
            1.0
        };
        let _ = progress.try_send(pct);
    }

    local_file
        .flush()
        .await
        .map_err(|e| AppError::msg(format!("flush local: {e}")))?;
    let _ = progress.try_send(1.0);

    Ok(())
}

/// Download a directory — walk the remote tree recursively, create local dirs,
/// download each file.
///
/// Progress = cumulative bytes downloaded / total bytes across all files.
async fn sftp_download_dir(
    sftp: &SftpChannel,
    remote_str: &str,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    /// Collect all files (remote_path, local_path, size) in the remote directory.
    async fn collect_files(
        sftp: &SftpChannel,
        remote: &str,
        local: &Path,
        files: &mut Vec<(String, PathBuf, u64)>,
    ) -> Result<()> {
        let read_dir = sftp.read_dir(remote).await.map_err(map_sftp_err)?;
        let mut entries = Vec::new();
        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            validate_remote_entry_name(&name)?;
            let metadata = entry.metadata();
            if metadata.is_symlink() {
                return Err(AppError::msg(format!(
                    "refusing to download remote symlink: {name:?}"
                )));
            }
            entries.push((name, metadata.is_dir(), metadata.size.unwrap_or(0)));
        }

        for (name, is_dir, size) in entries {
            let remote_child = if remote.ends_with('/') {
                format!("{remote}{name}")
            } else {
                format!("{remote}/{name}")
            };
            let local_child = safe_local_child(local, &name)?;
            if is_dir {
                Box::pin(collect_files(sftp, &remote_child, &local_child, files)).await?;
            } else {
                files.push((remote_child, local_child, size));
            }
        }
        Ok(())
    }

    // 1. Create and canonicalize the selected root before trusting remote names.
    tokio::fs::create_dir_all(local)
        .await
        .map_err(|e| AppError::msg(format!("create local dir: {e}")))?;
    let local_root = tokio::fs::canonicalize(local)
        .await
        .map_err(|e| AppError::msg(format!("canonicalize local dir: {e}")))?;

    // 2. Collect the file list + compute total size.
    let mut files: Vec<(String, PathBuf, u64)> = Vec::new();
    collect_files(sftp, remote_str, &local_root, &mut files).await?;
    let total_bytes: u64 = files.iter().map(|(_, _, s)| *s).sum();
    log::info!(
        "sftp_download_dir: \"{remote_str}\" → \"{}\" — {} files, {} bytes",
        local_root.display(),
        files.len(),
        total_bytes
    );

    // 3. Download each file, tracking cumulative progress.
    let mut bytes_done: u64 = 0;
    for (remote_path, local_path, _file_size) in &files {
        if cancel.is_cancelled() {
            log::info!("sftp_download_dir: cancelled at {bytes_done}/{total_bytes} bytes");
            let _ = progress.try_send(-1.0);
            return Err(AppError::msg("cancelled"));
        }

        log::debug!(
            "sftp_download_dir: downloading \"{remote_path}\" → \"{}\"",
            local_path.display()
        );

        // Create parents component-by-component and reject symlink traversal.
        create_safe_parent_dirs(&local_root, local_path).await?;

        let mut remote_file = sftp.open(remote_path).await.map_err(map_sftp_err)?;
        if let Ok(metadata) = tokio::fs::symlink_metadata(local_path).await {
            if metadata.file_type().is_symlink() {
                return Err(AppError::msg("refusing to overwrite a local symlink"));
            }
        }
        let mut local_file = tokio::fs::File::create(local_path)
            .await
            .map_err(|e| AppError::msg(format!("create local: {e}")))?;

        const CHUNK: usize = 32 * 1024;
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.is_cancelled() {
                log::info!("sftp_download_dir: cancelled mid-file at {bytes_done}/{total_bytes}");
                let _ = progress.try_send(-1.0);
                return Err(AppError::msg("cancelled"));
            }
            let n = remote_file
                .read(&mut buf)
                .await
                .map_err(|e| AppError::msg(format!("read remote: {e}")))?;
            if n == 0 {
                break;
            }
            local_file
                .write_all(&buf[..n])
                .await
                .map_err(|e| AppError::msg(format!("write local: {e}")))?;
            bytes_done += n as u64;
            let pct = if total_bytes > 0 {
                bytes_done as f64 / total_bytes as f64
            } else {
                1.0
            };
            let _ = progress.try_send(pct);
        }

        local_file
            .flush()
            .await
            .map_err(|e| AppError::msg(format!("flush local: {e}")))?;
    }

    let _ = progress.try_send(1.0);
    Ok(())
}

#[cfg(test)]
mod security_tests {
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
