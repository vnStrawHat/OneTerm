//! Incremental remote-to-local SFTP downloads.

use std::path::Path;

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, Result};

use crate::sftp_task::{
    create_safe_parent_dirs, map_sftp_err, safe_local_child, validate_remote_entry_name,
};

use super::staging::{finalize_local_file, temporary_local_sibling};
use super::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES};

/// Download a remote file or directory → local with progress reporting.
///
/// - File: open the remote file → read 32KB chunks → write the local file,
///   progress 0.0–1.0.
/// - Directory: walk the remote tree recursively → create local dirs → download
///   each file, progress = cumulative bytes / total bytes.
///
/// Checks `cancel.is_cancelled()` after each chunk read.
/// If cancelled → returns `Err(AppError::Cancelled)`.
pub(in crate::sftp_task) async fn sftp_download(
    sftp: &SftpChannel,
    remote: &Path,
    local: &Path,
    progress: &Sender<f64>,
    cancel: &CancellationToken,
) -> Result<()> {
    let remote_str = remote.to_string_lossy().replace('\\', "/");
    let attrs = sftp
        .symlink_metadata(&remote_str)
        .await
        .map_err(map_sftp_err)?;
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
    let attrs = sftp
        .symlink_metadata(remote_str)
        .await
        .map_err(map_sftp_err)?;
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
                return Err(AppError::Cancelled);
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
            return Err(AppError::Cancelled);
        }
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg(
                "remote download exceeded traversal depth limit",
            ));
        }

        for entry in sftp.read_dir(&remote).await.map_err(map_sftp_err)? {
            if cancel.is_cancelled() {
                let _ = progress.try_send(-1.0);
                return Err(AppError::Cancelled);
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
                        return Err(AppError::Cancelled);
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
