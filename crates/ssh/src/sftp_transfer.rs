//! SFTP upload/download orchestration and bounded traversal.
//!
//! The command loop stays in `sftp_task`; this module owns transfer-specific
//! streaming, cancellation, temporary-file finalization, and traversal plans.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, Result};

use super::{create_safe_parent_dirs, map_sftp_err, safe_local_child, validate_remote_entry_name};

pub(super) const MAX_TRAVERSAL_DEPTH: usize = 64;
pub(super) const MAX_TRAVERSAL_ENTRIES: usize = 100_000;

static TRANSFER_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn transfer_nonce() -> u64 {
    TRANSFER_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn temporary_local_sibling(target: &Path, marker: &str) -> Result<PathBuf> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .ok_or_else(|| AppError::msg("transfer target has no filename"))?
        .to_string_lossy();
    Ok(parent.join(format!(
        ".{name}.oneterm-{}-{}.{}",
        std::process::id(),
        transfer_nonce(),
        marker
    )))
}

fn temporary_remote_sibling(target: &str, marker: &str) -> Result<String> {
    let (parent, name) = target.rsplit_once('/').unwrap_or(("", target));
    if name.is_empty() {
        return Err(AppError::msg("remote transfer target has no filename"));
    }
    let temporary_name = format!(
        ".{name}.oneterm-{}-{}.{}",
        std::process::id(),
        transfer_nonce(),
        marker
    );
    Ok(if parent.is_empty() && target.starts_with('/') {
        format!("/{temporary_name}")
    } else if parent.is_empty() {
        temporary_name
    } else {
        format!("{parent}/{temporary_name}")
    })
}

async fn finalize_remote_file(sftp: &SftpChannel, temporary: &str, target: &str) -> Result<()> {
    let backup = temporary_remote_sibling(target, "backup")?;
    let had_target = match sftp.metadata(target).await {
        Ok(attributes) => {
            if attributes.is_dir() || attributes.is_symlink() {
                let _ = sftp.remove_file(temporary).await;
                return Err(AppError::msg(
                    "refusing to replace a remote directory or symlink",
                ));
            }
            if let Err(error) = sftp.rename(target, &backup).await {
                let _ = sftp.remove_file(temporary).await;
                return Err(map_sftp_err(error));
            }
            true
        }
        Err(_) => false,
    };

    if let Err(error) = sftp.rename(temporary, target).await {
        if had_target {
            let _ = sftp.rename(&backup, target).await;
        }
        let _ = sftp.remove_file(temporary).await;
        return Err(map_sftp_err(error));
    }

    if had_target {
        if let Err(error) = sftp.remove_file(&backup).await {
            log::warn!("failed to remove remote transfer backup {backup:?}: {error}");
        }
    }
    Ok(())
}

pub(super) async fn finalize_local_file(temporary: &Path, target: &Path) -> Result<()> {
    let backup = temporary_local_sibling(target, "backup")?;
    let had_target = match tokio::fs::symlink_metadata(target).await {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                let _ = tokio::fs::remove_file(temporary).await;
                return Err(AppError::msg(
                    "refusing to replace a local directory or symlink",
                ));
            }
            if let Err(error) = tokio::fs::rename(target, &backup).await {
                let _ = tokio::fs::remove_file(temporary).await;
                return Err(AppError::msg(format!("backup local target: {error}")));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            let _ = tokio::fs::remove_file(temporary).await;
            return Err(AppError::msg(format!("inspect local target: {error}")));
        }
    };

    if let Err(error) = tokio::fs::rename(temporary, target).await {
        if had_target {
            let _ = tokio::fs::rename(&backup, target).await;
        }
        let _ = tokio::fs::remove_file(temporary).await;
        return Err(AppError::msg(format!("finalize local download: {error}")));
    }

    if had_target {
        if let Err(error) = tokio::fs::remove_file(&backup).await {
            log::warn!(
                "failed to remove local transfer backup {}: {error}",
                backup.display()
            );
        }
    }
    Ok(())
}

/// Upload a local file or directory → remote with progress reporting.
///
/// - File: read contents → write 32KB chunks → report progress 0.0–1.0.
/// - Directory: walk recursively → create remote dirs → upload each file,
///   progress = cumulative bytes / total bytes.
///
/// Checks `cancel.is_cancelled()` after each chunk write.
/// If cancelled → returns `Err("cancelled")`.
pub(super) async fn sftp_upload(
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
    let total = tokio::fs::metadata(local)
        .await
        .map_err(|e| AppError::msg(format!("stat local: {e}")))?
        .len();
    let mut local_file = tokio::fs::File::open(local)
        .await
        .map_err(|e| AppError::msg(format!("open local: {e}")))?;

    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let temporary = temporary_remote_sibling(&remote_str, "part")?;
    let mut remote_file = sftp.create(&temporary).await.map_err(map_sftp_err)?;

    let transfer_result: Result<()> = async {
        const CHUNK: usize = 32 * 1024;
        let mut buffer = vec![0u8; CHUNK];
        let mut written: u64 = 0;
        loop {
            if cancel.is_cancelled() {
                log::info!("sftp_upload_file: cancelled at {written}/{total} bytes");
                let _ = progress.try_send(-1.0);
                return Err(AppError::msg("cancelled"));
            }
            let read = local_file
                .read(&mut buffer)
                .await
                .map_err(|e| AppError::msg(format!("read local: {e}")))?;
            if read == 0 {
                break;
            }
            remote_file
                .write_all(&buffer[..read])
                .await
                .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
            written += read as u64;
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
        Ok(())
    }
    .await;
    drop(remote_file);

    if let Err(error) = transfer_result {
        let _ = sftp.remove_file(&temporary).await;
        return Err(error);
    }
    finalize_remote_file(sftp, &temporary, &remote_str).await?;
    let _ = progress.try_send(1.0);
    Ok(())
}

#[derive(Debug)]
pub(super) enum LocalUploadEntry {
    Directory(PathBuf),
    File {
        local: PathBuf,
        remote: PathBuf,
        size: u64,
    },
}

fn send_local_upload_entry(
    entries: &Sender<LocalUploadEntry>,
    cancel: &CancellationToken,
    mut entry: LocalUploadEntry,
) -> Result<()> {
    loop {
        if cancel.is_cancelled() {
            return Err(AppError::msg("cancelled"));
        }
        match entries.try_send(entry) {
            Ok(()) => return Ok(()),
            Err(async_channel::TrySendError::Full(returned)) => {
                entry = returned;
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(async_channel::TrySendError::Closed(_)) => {
                return Err(AppError::msg("local upload consumer closed"));
            }
        }
    }
}

pub(super) fn stream_local_upload_entries(
    local_root: PathBuf,
    remote_root: PathBuf,
    cancel: CancellationToken,
    entries: &Sender<LocalUploadEntry>,
) -> Result<()> {
    let mut pending = VecDeque::from([(local_root, remote_root, 0usize)]);
    let mut visited = 0usize;

    while let Some((local, remote, depth)) = pending.pop_front() {
        if cancel.is_cancelled() {
            return Err(AppError::msg("cancelled"));
        }
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg("local upload exceeded traversal depth limit"));
        }
        send_local_upload_entry(
            entries,
            &cancel,
            LocalUploadEntry::Directory(remote.clone()),
        )?;

        for entry in std::fs::read_dir(&local)
            .map_err(|error| AppError::msg(format!("walk local dir: {error}")))?
        {
            let entry = entry.map_err(|error| AppError::msg(format!("walk local dir: {error}")))?;
            visited += 1;
            if visited > MAX_TRAVERSAL_ENTRIES {
                return Err(AppError::msg("local upload exceeded traversal entry limit"));
            }
            let path = entry.path();
            let remote_child = remote.join(entry.file_name());
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| AppError::msg(format!("stat local entry: {error}")))?;
            if metadata.file_type().is_symlink() {
                return Err(AppError::msg(format!(
                    "refusing to upload local symlink: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                pending.push_back((path, remote_child, depth + 1));
            } else if metadata.is_file() {
                send_local_upload_entry(
                    entries,
                    &cancel,
                    LocalUploadEntry::File {
                        local: path,
                        remote: remote_child,
                        size: metadata.len(),
                    },
                )?;
            }
        }
    }
    Ok(())
}

/// Upload a directory incrementally: discovery sends a bounded stream of directories and files,
/// while the SFTP task creates directories and transfers files as soon as they are discovered.
/// Progress is monotonic and remains below completion while discovery continues.
async fn sftp_upload_dir(
    sftp: &SftpChannel,
    local: &Path,
    remote: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    const DISCOVERY_CAPACITY: usize = 128;
    let (entries_tx, entries_rx) = async_channel::bounded(DISCOVERY_CAPACITY);
    let local_root = local.to_path_buf();
    let remote_root = remote.to_path_buf();
    let traversal = tokio::task::spawn_blocking({
        let cancel = cancel.clone();
        move || {
            let result = stream_local_upload_entries(local_root, remote_root, cancel, &entries_tx);
            drop(entries_tx);
            result
        }
    });

    let mut discovered_files = 0usize;
    let mut discovered_bytes = 0u64;
    let mut bytes_done = 0u64;
    let mut reported_progress = 0.0f64;
    macro_rules! fail_upload {
        ($error:expr) => {{
            cancel.cancel();
            let _ = traversal.await;
            return Err($error);
        }};
    }

    while let Ok(entry) = entries_rx.recv().await {
        match entry {
            LocalUploadEntry::Directory(dir) => {
                if cancel.is_cancelled() {
                    let _ = progress.try_send(-1.0);
                    let _ = traversal.await;
                    return Err(AppError::msg("cancelled"));
                }
                let dir_str = dir.to_string_lossy().replace('\\', "/");
                if let Err(create_error) = sftp.create_dir(&dir_str).await {
                    match sftp.metadata(&dir_str).await {
                        Ok(attributes) if attributes.is_dir() && !attributes.is_symlink() => {}
                        Ok(_) => {
                            cancel.cancel();
                            let _ = traversal.await;
                            return Err(AppError::msg(format!(
                                "remote upload path exists but is not a directory: {dir_str}"
                            )));
                        }
                        Err(metadata_error) => {
                            cancel.cancel();
                            let _ = traversal.await;
                            return Err(AppError::msg(format!(
                                "create remote directory {dir_str}: {create_error}; verify existing path: {metadata_error}"
                            )));
                        }
                    }
                }
            }
            LocalUploadEntry::File {
                local: local_path,
                remote: remote_path,
                size: file_size,
            } => {
                discovered_files += 1;
                discovered_bytes = discovered_bytes.saturating_add(file_size);
                log::debug!(
                    "sftp_upload_dir: uploading discovered file \"{}\" → \"{}\" ({file_size} bytes)",
                    local_path.display(),
                    remote_path.display()
                );
                let mut local_file = match tokio::fs::File::open(&local_path).await {
                    Ok(file) => file,
                    Err(error) => fail_upload!(AppError::msg(format!("open local: {error}"))),
                };
                let remote_str = remote_path.to_string_lossy().replace('\\', "/");
                let temporary = match temporary_remote_sibling(&remote_str, "part") {
                    Ok(path) => path,
                    Err(error) => fail_upload!(error),
                };
                let mut remote_file = match sftp.create(&temporary).await.map_err(map_sftp_err) {
                    Ok(file) => file,
                    Err(error) => fail_upload!(error),
                };

                let transfer_result: Result<()> = async {
                    const CHUNK: usize = 32 * 1024;
                    let mut buffer = vec![0u8; CHUNK];
                    loop {
                        if cancel.is_cancelled() {
                            let _ = progress.try_send(-1.0);
                            return Err(AppError::msg("cancelled"));
                        }
                        let read = local_file
                            .read(&mut buffer)
                            .await
                            .map_err(|e| AppError::msg(format!("read local: {e}")))?;
                        if read == 0 {
                            break;
                        }
                        remote_file
                            .write_all(&buffer[..read])
                            .await
                            .map_err(|e| AppError::msg(format!("write remote: {e}")))?;
                        bytes_done += read as u64;
                        let denominator = discovered_bytes.max(bytes_done);
                        let candidate = if denominator > 0 {
                            (bytes_done as f64 / denominator as f64).min(0.99)
                        } else {
                            0.0
                        };
                        if candidate > reported_progress {
                            reported_progress = candidate;
                            let _ = progress.try_send(reported_progress);
                        }
                    }
                    remote_file
                        .flush()
                        .await
                        .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
                    Ok(())
                }
                .await;
                drop(remote_file);

                if let Err(error) = transfer_result {
                    let _ = sftp.remove_file(&temporary).await;
                    cancel.cancel();
                    let _ = traversal.await;
                    return Err(error);
                }
                if let Err(error) = finalize_remote_file(sftp, &temporary, &remote_str).await {
                    cancel.cancel();
                    let _ = traversal.await;
                    return Err(error);
                }
            }
        }
    }

    let traversal_result = traversal
        .await
        .map_err(|error| AppError::msg(format!("walk local dir task: {error}")))?;
    traversal_result?;
    log::info!(
        "sftp_upload_dir: \"{}\" → \"{}\" — {discovered_files} files, {discovered_bytes} bytes",
        local.display(),
        remote.display()
    );
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
pub(super) async fn sftp_download(
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

    let temporary = temporary_local_sibling(local, "part")?;
    let mut local_file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|e| AppError::msg(format!("create local temporary file: {e}")))?;

    let transfer_result: Result<()> = async {
        const CHUNK: usize = 32 * 1024;
        let mut buf = vec![0u8; CHUNK];
        let mut read: u64 = 0;
        loop {
            if cancel.is_cancelled() {
                log::info!("sftp_download_file: cancelled at {read}/{total} bytes");
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
        local_file
            .sync_all()
            .await
            .map_err(|e| AppError::msg(format!("sync local: {e}")))?;
        Ok(())
    }
    .await;
    drop(local_file);

    if let Err(error) = transfer_result {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    finalize_local_file(&temporary, local).await?;
    let _ = progress.try_send(1.0);
    Ok(())
}

/// Download a directory — traverse the remote tree incrementally and download files as discovered.
///
/// Discovery uses a bounded depth-first work stack instead of materializing the complete
/// file tree. Progress is monotonic and remains indeterminate-ish until discovery completes.
async fn sftp_download_dir(
    sftp: &SftpChannel,
    remote_str: &str,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    // Create and canonicalize the selected root before trusting remote names.
    tokio::fs::create_dir_all(local)
        .await
        .map_err(|e| AppError::msg(format!("create local dir: {e}")))?;
    let local_root = tokio::fs::canonicalize(local)
        .await
        .map_err(|e| AppError::msg(format!("canonicalize local dir: {e}")))?;

    // A depth-first stack keeps discovery memory proportional to pending directories,
    // while the entry and depth caps continue to bound hostile remote trees.
    let mut pending = vec![(remote_str.to_string(), local_root.clone(), 0usize)];
    let mut visited = 0usize;
    let mut discovered_files = 0usize;
    let mut discovered_bytes = 0u64;
    let mut bytes_done = 0u64;
    let mut reported_progress = 0.0f64;

    while let Some((remote, local_dir, depth)) = pending.pop() {
        if cancel.is_cancelled() {
            let _ = progress.try_send(-1.0);
            return Err(AppError::msg("cancelled"));
        }
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg(
                "remote download exceeded traversal depth limit",
            ));
        }

        for entry in sftp.read_dir(&remote).await.map_err(map_sftp_err)? {
            if cancel.is_cancelled() {
                let _ = progress.try_send(-1.0);
                return Err(AppError::msg("cancelled"));
            }
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            validate_remote_entry_name(&name)?;
            visited += 1;
            if visited > MAX_TRAVERSAL_ENTRIES {
                return Err(AppError::msg(
                    "remote download exceeded traversal entry limit",
                ));
            }

            let metadata = entry.metadata();
            if metadata.is_symlink() {
                return Err(AppError::msg(format!(
                    "refusing to download remote symlink: {name:?}"
                )));
            }
            let remote_child = format!("{}/{}", remote.trim_end_matches('/'), name);
            let local_child = safe_local_child(&local_dir, &name)?;
            if metadata.is_dir() {
                // Create empty directories as they are discovered, preserving the
                // original tree without retaining a directory plan.
                create_safe_parent_dirs(
                    &local_root,
                    &local_child.join(".oneterm-directory-placeholder"),
                )
                .await?;
                pending.push((remote_child, local_child, depth + 1));
                continue;
            }

            discovered_files += 1;
            discovered_bytes = discovered_bytes.saturating_add(metadata.size.unwrap_or(0));
            log::debug!(
                "sftp_download_dir: downloading discovered file \"{remote_child}\" → \"{}\"",
                local_child.display()
            );
            create_safe_parent_dirs(&local_root, &local_child).await?;

            let mut remote_file = sftp.open(&remote_child).await.map_err(map_sftp_err)?;
            if let Ok(existing) = tokio::fs::symlink_metadata(&local_child).await {
                if existing.file_type().is_symlink() {
                    return Err(AppError::msg("refusing to overwrite a local symlink"));
                }
            }
            let temporary = temporary_local_sibling(&local_child, "part")?;
            let mut local_file = tokio::fs::File::create(&temporary)
                .await
                .map_err(|e| AppError::msg(format!("create local temporary file: {e}")))?;

            let transfer_result: Result<()> = async {
                const CHUNK: usize = 32 * 1024;
                let mut buf = vec![0u8; CHUNK];
                loop {
                    if cancel.is_cancelled() {
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
                    let denominator = discovered_bytes.max(bytes_done);
                    let candidate = if denominator > 0 {
                        (bytes_done as f64 / denominator as f64).min(0.99)
                    } else {
                        0.0
                    };
                    if candidate > reported_progress {
                        reported_progress = candidate;
                        let _ = progress.try_send(reported_progress);
                    }
                }
                local_file
                    .flush()
                    .await
                    .map_err(|e| AppError::msg(format!("flush local: {e}")))?;
                local_file
                    .sync_all()
                    .await
                    .map_err(|e| AppError::msg(format!("sync local: {e}")))?;
                Ok(())
            }
            .await;
            drop(local_file);

            if let Err(error) = transfer_result {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(error);
            }
            if let Err(error) = finalize_local_file(&temporary, &local_child).await {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(error);
            }
        }
    }

    log::info!(
        "sftp_download_dir: \"{remote_str}\" → \"{}\" — {discovered_files} files, {discovered_bytes} bytes",
        local_root.display()
    );
    let _ = progress.try_send(1.0);
    Ok(())
}
