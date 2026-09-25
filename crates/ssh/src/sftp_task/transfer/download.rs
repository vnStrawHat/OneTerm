//! Remote-to-local SFTP downloads: one file, or a folder listed in full first.

use std::path::{Path, PathBuf};

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use russh_sftp::protocol::FileAttributes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, RemotePath, Result, TransferEvent, report_best_effort};

use crate::sftp_task::{
    create_safe_parent_dirs, map_sftp_err, safe_local_child, validate_remote_entry_name,
};

use super::pipeline::copy_sequential;
use super::staging::{finalize_local_file, temporary_local_sibling};
use super::{
    MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES, apply_remote_metadata_to_local,
    report_cancellation, send_progress,
};

/// Download a remote file or directory → local with progress reporting.
///
/// - File: one sequential pass over a single remote handle, pipelined by
///   russh-sftp (see [`super::pipeline`]), into a temporary sibling that
///   replaces the target only once complete; progress 0.0–1.0.
/// - Directory: list the whole remote tree (creating local dirs), then download
///   each file; progress = cumulative bytes / bytes of the whole tree.
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
            send_progress(progress, TransferEvent::Progress(fraction));
        };
        download_file_contents(sftp, remote_str, &attrs, local, cancel, &mut on_bytes)
            .await
            .map_err(|error| report_cancellation(progress, error))?;
        send_progress(progress, TransferEvent::Progress(1.0));
        Ok(())
    }
}

/// Copy one remote file (whose attributes were just read as `source`) to
/// `local`.
///
/// The bytes land in a temporary sibling first and replace `local` atomically
/// on success, then the remote permissions/times are applied (SEC-15).
/// `on_bytes` receives the running byte count roughly once per chunk.
///
/// One handle, read straight through: russh-sftp keeps `max_concurrent_reads`
/// READ packets on the wire by itself, and seeking would throw that read-ahead
/// away (`IN-0037`). `take(total)` stops at the size `stat` announced; a file
/// that shrank in the meantime ends earlier at EOF, and a size-less file
/// (`total == 0`, which a genuinely empty file also yields) reads to EOF.
async fn download_file_contents(
    sftp: &SftpChannel,
    remote_str: &str,
    source: &FileAttributes,
    local: &Path,
    cancel: &CancellationToken,
    on_bytes: &mut impl FnMut(u64),
) -> Result<()> {
    let total = source.size.unwrap_or(0);
    if let Ok(metadata) = tokio::fs::symlink_metadata(local).await {
        if metadata.file_type().is_symlink() {
            return Err(AppError::msg("refusing to overwrite a local symlink"));
        }
    }

    let announced = if total > 0 { total } else { u64::MAX };
    let temporary = temporary_local_sibling(local, "part")?;
    let mut reader = sftp
        .open(remote_str)
        .await
        .map_err(map_sftp_err)?
        .take(announced);

    let transfer_result: Result<()> = async {
        let mut local_file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|e| AppError::msg(format!("create local temporary file: {e}")))?;
        copy_sequential(&mut reader, &mut local_file, cancel, on_bytes).await?;
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
    // Only an awaited close gives the handle back to russh-sftp's open-handle
    // count; dropping the `File` closes it on the server but leaks the count
    // until `limits@openssh.com` refuses every later open (BUG-0076). Closing a
    // read handle cannot change bytes already synced, so a failure is logged.
    // After a failed or cancelled copy the CLOSE reply queues behind up to the
    // whole read-ahead budget, so it is awaited off the caller's path: a cancel
    // must not wait for ~4 MB to cross a slow link.
    let remote_file = reader.into_inner();
    if transfer_result.is_ok() {
        report_best_effort(
            "sftp download: close remote file",
            remote_file.close().await,
        );
    } else {
        tokio::spawn(async move {
            report_best_effort(
                "sftp download: close remote file after failed copy",
                remote_file.close().await,
            );
        });
    }

    if let Err(error) = transfer_result {
        report_best_effort(
            "sftp download: remove temporary after failed copy",
            tokio::fs::remove_file(&temporary).await,
        );
        return Err(error);
    }
    if let Err(error) = finalize_local_file(&temporary, local).await {
        report_best_effort(
            "sftp download: remove temporary after failed finalize",
            tokio::fs::remove_file(&temporary).await,
        );
        return Err(error);
    }
    apply_remote_metadata_to_local(local, source).await;
    Ok(())
}

/// Download a directory: list the whole remote tree first, then download its
/// files in discovery order.
///
/// Listing first gives progress a true denominator (`BUG-0077`): dividing by the
/// bytes discovered so far read ~0.99 after the first file. While listing, one
/// `TransferEvent::Discovering(files_found)` goes out per directory read. The
/// file list is bounded by the entry and depth caps, which also bound hostile
/// remote trees.
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

    // Pass 1: discovery. Depth-first; local directories (empty ones included)
    // are created as they are found.
    let mut pending = vec![(remote_str.to_string(), local_root.clone(), 0usize)];
    let mut visited = 0usize;
    let mut files: Vec<(String, PathBuf, FileAttributes)> = Vec::new();
    let mut total_bytes = 0u64;

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
                create_safe_parent_dirs(
                    &local_root,
                    &local_child.join(".oneterm-directory-placeholder"),
                )
                .await?;
                pending.push((remote_child, local_child, depth + 1));
            } else {
                total_bytes = total_bytes.saturating_add(metadata.size.unwrap_or(0));
                files.push((remote_child, local_child, metadata.clone()));
            }
        }
        send_progress(progress, TransferEvent::Discovering(files.len()));
    }

    // Pass 2: download; progress = bytes done / bytes of the whole tree.
    let mut bytes_done = 0u64;
    let mut reported_progress = 0.0f64;
    for (remote_child, local_child, source) in &files {
        log::debug!(
            "sftp_download_dir: downloading \"{remote_child}\" → \"{}\"",
            local_child.display()
        );
        create_safe_parent_dirs(&local_root, local_child).await?;

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
        download_file_contents(
            sftp,
            remote_child,
            source,
            local_child,
            cancel,
            &mut on_bytes,
        )
        .await
        .map_err(|error| report_cancellation(progress, error))?;
    }

    log::info!(
        "sftp_download_dir: \"{remote_str}\" → \"{}\" — {} files, {total_bytes} bytes",
        local_root.display(),
        files.len()
    );
    send_progress(progress, TransferEvent::Progress(1.0));
    Ok(())
}
