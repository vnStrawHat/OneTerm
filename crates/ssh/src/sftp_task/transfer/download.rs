//! Incremental remote-to-local SFTP downloads.

use std::path::Path;

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, RemotePath, Result, TransferEvent};

use crate::sftp_task::{
    create_safe_parent_dirs, map_sftp_err, safe_local_child, validate_remote_entry_name,
};

use super::pipeline::{copy_sequential, copy_striped, read_handles_for};
use super::staging::{finalize_local_file, temporary_local_sibling};
use super::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES, report_cancellation};

/// Download a remote file or directory → local with progress reporting.
///
/// - File: pipelined chunk reads (see [`super::pipeline`]) into a temporary
///   sibling that replaces the target only once complete; progress 0.0–1.0.
/// - Directory: walk the remote tree recursively → create local dirs → download
///   each file, progress = cumulative bytes / bytes discovered so far.
///
/// Cancellation is observed between chunks; a cancelled transfer emits
/// `TransferEvent::Cancelled` and returns `Err(AppError::Cancelled)`.
pub(in crate::sftp_task) async fn sftp_download(
    sftp: &SftpChannel,
    remote: &RemotePath,
    local: &Path,
    progress: &Sender<TransferEvent>,
    cancel: &CancellationToken,
) -> Result<()> {
    let remote_str = remote.as_str();
    let attrs = sftp
        .symlink_metadata(remote_str)
        .await
        .map_err(map_sftp_err)?;
    if attrs.is_symlink() {
        return Err(AppError::msg("refusing to download a remote symlink"));
    }

    if attrs.is_dir() {
        sftp_download_dir(sftp, remote_str, local, progress, cancel).await
    } else {
        let total = attrs.size.unwrap_or(0);
        let mut on_bytes = |done: u64| {
            let fraction = if total > 0 {
                (done as f64 / total as f64).min(1.0)
            } else {
                1.0
            };
            let _ = progress.try_send(TransferEvent::Progress(fraction));
        };
        download_file_contents(sftp, remote_str, total, local, cancel, &mut on_bytes)
            .await
            .map_err(|error| report_cancellation(progress, error))?;
        let _ = progress.try_send(TransferEvent::Progress(1.0));
        Ok(())
    }
}

/// Copy one remote file (whose size was just read as `total`) to `local`.
///
/// The bytes land in a temporary sibling first and replace `local` atomically
/// on success. `on_bytes` receives the running byte count after every chunk.
/// Files with a known size use striped, pipelined reads; size-less files fall
/// back to one sequential handle read to EOF.
async fn download_file_contents(
    sftp: &SftpChannel,
    remote_str: &str,
    total: u64,
    local: &Path,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> Result<()> {
    if let Ok(metadata) = tokio::fs::symlink_metadata(local).await {
        if metadata.file_type().is_symlink() {
            return Err(AppError::msg("refusing to overwrite a local symlink"));
        }
    }

    let mut readers = Vec::with_capacity(read_handles_for(total));
    for _ in 0..read_handles_for(total) {
        readers.push(sftp.open(remote_str).await.map_err(map_sftp_err)?);
    }

    let temporary = temporary_local_sibling(local, "part")?;
    let mut local_file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|e| AppError::msg(format!("create local temporary file: {e}")))?;

    let transfer_result: Result<()> = async {
        if total > 0 {
            copy_striped(readers, total, &mut local_file, cancel, on_bytes).await?;
        } else {
            let mut reader = readers
                .into_iter()
                .next()
                .ok_or_else(|| AppError::msg("download opened no remote handle"))?;
            copy_sequential(&mut reader, &mut local_file, cancel, on_bytes).await?;
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
    if let Err(error) = finalize_local_file(&temporary, local).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
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
    progress: &Sender<TransferEvent>,
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
            return Err(report_cancellation(progress, AppError::Cancelled));
        }
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg(
                "remote download exceeded traversal depth limit",
            ));
        }

        for entry in sftp.read_dir(&remote).await.map_err(map_sftp_err)? {
            if cancel.is_cancelled() {
                return Err(report_cancellation(progress, AppError::Cancelled));
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

            let file_size = metadata.size.unwrap_or(0);
            discovered_files += 1;
            discovered_bytes = discovered_bytes.saturating_add(file_size);
            log::debug!(
                "sftp_download_dir: downloading discovered file \"{remote_child}\" → \"{}\"",
                local_child.display()
            );
            create_safe_parent_dirs(&local_root, &local_child).await?;

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
                    let _ = progress.try_send(TransferEvent::Progress(reported_progress));
                }
            };
            download_file_contents(
                sftp,
                &remote_child,
                file_size,
                &local_child,
                cancel,
                &mut on_bytes,
            )
            .await
            .map_err(|error| report_cancellation(progress, error))?;
        }
    }

    log::info!(
        "sftp_download_dir: \"{remote_str}\" → \"{}\" — {discovered_files} files, {discovered_bytes} bytes",
        local_root.display()
    );
    let _ = progress.try_send(TransferEvent::Progress(1.0));
    Ok(())
}
