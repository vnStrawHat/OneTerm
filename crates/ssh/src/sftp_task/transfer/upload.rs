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
/// - Directory: list the local tree, then create remote dirs and upload each
///   file; progress = cumulative bytes / bytes of the whole tree.
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
    // Only an awaited close gives the handle back to russh-sftp's open-handle
    // count (BUG-0076). The close also drains write acknowledgements and a
    // server may report a deferred write error there, so it fails the upload.
    // It is awaited on the failure path too: the remove below must reach the
    // server after the CLOSE, as the dropped handle's CLOSE used to.
    let closed = remote_file.close().await;
    let transfer_result = match transfer_result {
        Ok(()) => closed.map_err(|e| AppError::msg(format!("close remote: {e}"))),
        Err(error) => {
            report_best_effort(
                "sftp upload: close remote temporary after failed copy",
                closed,
            );
            Err(error)
        }
    };

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

/// Walk the local tree breadth-first and list what the upload will create:
/// each directory before its contents. Runs on the blocking pool; the depth and
/// entry caps bound the list.
pub(in crate::sftp_task) fn collect_local_upload_entries(
    local_root: PathBuf,
    remote_root: RemotePath,
    cancel: &CancellationToken,
) -> Result<Vec<LocalUploadEntry>> {
    let mut pending = VecDeque::from([(local_root, remote_root, 0usize)]);
    let mut visited = 0usize;
    let mut entries = Vec::new();

    while let Some((local, remote, depth)) = pending.pop_front() {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg("local upload exceeded traversal depth limit"));
        }
        entries.push(LocalUploadEntry::Directory(remote.clone()));

        for entry in std::fs::read_dir(&local)
            .map_err(|error| AppError::msg(format!("walk local dir: {error}")))?
        {
            if cancel.is_cancelled() {
                return Err(AppError::Cancelled);
            }
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
                entries.push(LocalUploadEntry::File {
                    local: path,
                    remote: remote_child,
                    size: metadata.len(),
                });
            }
        }
    }
    Ok(entries)
}

/// Upload a directory: list the local tree first, then create each remote
/// directory and upload each file in walk order.
///
/// Listing first gives progress a true denominator (`BUG-0077`): dividing by the
/// bytes discovered so far read ~0.99 after the first file.
async fn sftp_upload_dir(
    sftp: &SftpChannel,
    local: &Path,
    remote: &RemotePath,
    progress: &Sender<TransferEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    let entries = tokio::task::spawn_blocking({
        let (local_root, remote_root, cancel) =
            (local.to_path_buf(), remote.clone(), cancel.clone());
        move || collect_local_upload_entries(local_root, remote_root, &cancel)
    })
    .await
    .map_err(|error| AppError::msg(format!("walk local dir task: {error}")))?
    .map_err(|error| report_cancellation(progress, error))?;

    let (mut files, mut total_bytes) = (0usize, 0u64);
    for entry in &entries {
        if let LocalUploadEntry::File { size, .. } = entry {
            files += 1;
            total_bytes = total_bytes.saturating_add(*size);
        }
    }

    let mut bytes_done = 0u64;
    let mut reported_progress = 0.0f64;
    for entry in entries {
        match entry {
            LocalUploadEntry::Directory(dir) => {
                if cancel.is_cancelled() {
                    return Err(report_cancellation(progress, AppError::Cancelled));
                }
                let dir_str = dir.as_str();
                if let Err(create_error) = sftp.create_dir(dir_str).await {
                    match sftp.symlink_metadata(dir_str).await {
                        Ok(attributes) if attributes.is_dir() && !attributes.is_symlink() => {}
                        Ok(_) => {
                            return Err(AppError::msg(format!(
                                "remote upload path exists but is not a directory: {dir_str}"
                            )));
                        }
                        Err(metadata_error) => {
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
                log::debug!(
                    "sftp_upload_dir: uploading \"{}\" → \"{remote_path}\" ({file_size} bytes)",
                    local_path.display()
                );
                let file_start = bytes_done;
                let mut on_bytes = |done: u64| {
                    bytes_done = file_start + done;
                    if total_bytes > 0 {
                        // `min`: a file that grew since it was listed.
                        let fraction = (bytes_done as f64 / total_bytes as f64).min(1.0);
                        if fraction > reported_progress {
                            reported_progress = fraction;
                            send_progress(progress, TransferEvent::Progress(fraction));
                        }
                    }
                };
                upload_file_contents(
                    sftp,
                    &local_path,
                    remote_path.as_str(),
                    cancel,
                    &mut on_bytes,
                )
                .await
                .map_err(|error| report_cancellation(progress, error))?;
            }
        }
    }

    log::info!(
        "sftp_upload_dir: \"{}\" → \"{remote}\" — {files} files, {total_bytes} bytes",
        local.display()
    );
    send_progress(progress, TransferEvent::Progress(1.0));
    Ok(())
}
