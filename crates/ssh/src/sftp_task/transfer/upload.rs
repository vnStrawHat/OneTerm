//! Incremental local-to-remote SFTP uploads.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, Result};

use crate::sftp_task::map_sftp_err;

use super::staging::{finalize_remote_file, temporary_remote_sibling};
use super::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES};

/// Upload a local file or directory → remote with progress reporting.
///
/// - File: read contents → write 32KB chunks → report progress 0.0–1.0.
/// - Directory: walk recursively → create remote dirs → upload each file,
///   progress = cumulative bytes / total bytes.
///
/// Checks `cancel.is_cancelled()` after each chunk write.
/// If cancelled → returns `Err(AppError::Cancelled)`.
pub(in crate::sftp_task) async fn sftp_upload(
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
                return Err(AppError::Cancelled);
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
pub(in crate::sftp_task) enum LocalUploadEntry {
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
            return Err(AppError::Cancelled);
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

pub(in crate::sftp_task) fn stream_local_upload_entries(
    local_root: PathBuf,
    remote_root: PathBuf,
    cancel: CancellationToken,
    entries: &Sender<LocalUploadEntry>,
) -> Result<()> {
    let mut pending = VecDeque::from([(local_root, remote_root, 0usize)]);
    let mut visited = 0usize;

    while let Some((local, remote, depth)) = pending.pop_front() {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
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
                    return Err(AppError::Cancelled);
                }
                let dir_str = dir.to_string_lossy().replace('\\', "/");
                if let Err(create_error) = sftp.create_dir(&dir_str).await {
                    match sftp.symlink_metadata(&dir_str).await {
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
                            return Err(AppError::Cancelled);
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
