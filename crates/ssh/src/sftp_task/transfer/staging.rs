//! Temporary-file staging and atomic transfer finalization.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use russh_sftp::client::SftpSession as SftpChannel;

use oneterm_core::{AppError, Result};

use crate::sftp_task::map_sftp_err;

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

pub(super) fn temporary_remote_sibling(target: &str, marker: &str) -> Result<String> {
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

pub(super) async fn finalize_remote_file(
    sftp: &SftpChannel,
    temporary: &str,
    target: &str,
) -> Result<()> {
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
