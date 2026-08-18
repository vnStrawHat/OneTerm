//! Incremental local-to-remote SFTP uploads.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, RemotePath, Result, TransferEvent, report_best_effort};

use crate::sftp_task::map_sftp_err;

use super::pipeline::copy_sequential;
use super::staging::{finalize_remote_file, temporary_remote_sibling};
use super::{
    MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES, apply_local_metadata_to_remote,
    report_cancellation, send_progress,
};

/// Upload a local file or directory → remote with progress reporting.
///
/// - File: chunked writes (see [`super::pipeline`]; the remote handle keeps
///   several writes in flight) into a temporary sibling that replaces the target
///   once complete; progress 0.0–1.0.
/// - Directory: walk recursively → create remote dirs → upload each file,
///   progress = cumulative bytes / bytes discovered so far.
///
/// Cancellation is observed between chunks; a cancelled transfer emits
/// `TransferEvent::Cancelled` and returns `Err(AppError::Cancelled)`.
pub(in crate::sftp_task) async fn sftp_upload(
    sftp: &SftpChannel,
    local: &Path,
    remote: &RemotePath,
    progress: &Sender<TransferEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    let metadata = tokio::fs::metadata(local)
        .await
        .map_err(|e| AppError::msg(format!("stat local: {e}")))?;

    if metadata.is_dir() {
        sftp_upload_dir(sftp, local, remote, progress, cancel).await
    } else {
        let total = metadata.len();
        let mut on_bytes = |done: u64| {
            let fraction = if total > 0 {
                (done as f64 / total as f64).min(1.0)
            } else {
                1.0
            };
            send_progress(progress, TransferEvent::Progress(fraction));
        };
        upload_file_contents(sftp, local, remote.as_str(), cancel, &mut on_bytes)
            .await
            .map_err(|error| report_cancellation(progress, error))?;
        send_progress(progress, TransferEvent::Progress(1.0));
        Ok(())
    }
}

/// Copy one local file to `remote_str`.
///
/// The bytes land in a temporary remote sibling first and replace the target
/// atomically on success, then the local permissions/times are applied to the
/// remote file (SEC-15). `on_bytes` receives the running byte count after
/// every chunk.
async fn upload_file_contents(
    sftp: &SftpChannel,
    local: &Path,
    remote_str: &str,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> Result<()> {
    let mut local_file = tokio::fs::File::open(local)
        .await
        .map_err(|e| AppError::msg(format!("open local: {e}")))?;
    let local_metadata = local_file
        .metadata()
        .await
        .map_err(|e| AppError::msg(format!("stat local: {e}")))?;

    let temporary = temporary_remote_sibling(remote_str, "part")?;
    let mut remote_file = sftp.create(&temporary).await.map_err(map_sftp_err)?;

    let transfer_result: Result<()> = async {
        copy_sequential(&mut local_file, &mut remote_file, cancel, on_bytes).await?;
        remote_file
            .flush()
            .await
            .map_err(|e| AppError::msg(format!("flush remote: {e}")))?;
        Ok(())
    }
    .await;
    drop(remote_file);

    if let Err(error) = transfer_result {
        report_best_effort(
            "sftp upload: remove remote temporary after failed copy",
            sftp.remove_file(&temporary).await,
        );
        return Err(error);
    }
    finalize_remote_file(sftp, &temporary, remote_str).await?;
    apply_local_metadata_to_remote(sftp, remote_str, &local_metadata).await;
    Ok(())
}

#[derive(Debug)]
pub(in crate::sftp_task) enum LocalUploadEntry {
    Directory(RemotePath),
    File {
        local: PathBuf,
        remote: RemotePath,
        size: u64,
    },
}

/// Hand one discovered entry to the upload loop. Runs on the blocking pool,
/// so it parks on the bounded channel instead of spinning (CORR-18); the
/// consumer drops its receiver when it stops (cancel or failure), which
/// unblocks the walker with `Closed`.
fn send_local_upload_entry(
    entries: &Sender<LocalUploadEntry>,
    cancel: &CancellationToken,
    entry: LocalUploadEntry,
) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    entries.send_blocking(entry).map_err(|_| {
        if cancel.is_cancelled() {
            AppError::Cancelled
        } else {
            AppError::msg("local upload consumer closed")
        }
    })
}

pub(in crate::sftp_task) fn stream_local_upload_entries(
    local_root: PathBuf,
    remote_root: RemotePath,
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
            let remote_child = remote.join(&entry.file_name().to_string_lossy());
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
    remote: &RemotePath,
    progress: &Sender<TransferEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    const DISCOVERY_CAPACITY: usize = 128;
    let (entries_tx, entries_rx) = async_channel::bounded(DISCOVERY_CAPACITY);
    let local_root = local.to_path_buf();
    let remote_root = remote.clone();
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

    /// Stop the walker: cancel, drop our end of the discovery channel so a
    /// walker parked in `send_blocking` wakes up, then wait for it to exit.
    /// The walker's own result is irrelevant here — the caller already has
    /// the error that ended the upload.
    async fn stop_traversal(
        cancel: &CancellationToken,
        entries_rx: async_channel::Receiver<LocalUploadEntry>,
        traversal: tokio::task::JoinHandle<Result<()>>,
    ) {
        cancel.cancel();
        drop(entries_rx);
        if let Err(join_error) = traversal.await {
            log::warn!("sftp_upload_dir: local walker task failed: {join_error}");
        }
    }

    loop {
        let entry = match entries_rx.recv().await {
            Ok(entry) => entry,
            Err(_) => break,
        };
        match entry {
            LocalUploadEntry::Directory(dir) => {
                if cancel.is_cancelled() {
                    send_progress(progress, TransferEvent::Cancelled);
                    stop_traversal(cancel, entries_rx, traversal).await;
                    return Err(AppError::Cancelled);
                }
                let dir_str = dir.as_str();
                if let Err(create_error) = sftp.create_dir(dir_str).await {
                    match sftp.symlink_metadata(dir_str).await {
                        Ok(attributes) if attributes.is_dir() && !attributes.is_symlink() => {}
                        Ok(_) => {
                            let error = AppError::msg(format!(
                                "remote upload path exists but is not a directory: {dir_str}"
                            ));
                            stop_traversal(cancel, entries_rx, traversal).await;
                            return Err(error);
                        }
                        Err(metadata_error) => {
                            let error = AppError::msg(format!(
                                "create remote directory {dir_str}: {create_error}; verify existing path: {metadata_error}"
                            ));
                            stop_traversal(cancel, entries_rx, traversal).await;
                            return Err(error);
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
                    "sftp_upload_dir: uploading discovered file \"{}\" → \"{remote_path}\" ({file_size} bytes)",
                    local_path.display()
                );
                let file_start = bytes_done;
                let mut on_bytes = |done: u64| {
                    bytes_done = file_start + done;
                    let denominator = discovered_bytes.max(bytes_done);
                    let candidate = if denominator > 0 {
                        (bytes_done as f64 / denominator as f64).min(0.99)
                    } else {
                        0.0
                    };
                    if candidate > reported_progress {
                        reported_progress = candidate;
                        send_progress(progress, TransferEvent::Progress(reported_progress));
                    }
                };
                let result = upload_file_contents(
                    sftp,
                    &local_path,
                    remote_path.as_str(),
                    cancel,
                    &mut on_bytes,
                )
                .await;
                if let Err(error) = result {
                    let error = report_cancellation(progress, error);
                    stop_traversal(cancel, entries_rx, traversal).await;
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
        "sftp_upload_dir: \"{}\" → \"{remote}\" — {discovered_files} files, {discovered_bytes} bytes",
        local.display()
    );
    send_progress(progress, TransferEvent::Progress(1.0));
    Ok(())
}
