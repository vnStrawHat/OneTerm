//! SFTP upload/download orchestration and bounded traversal.

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use async_channel::Sender;
use russh_sftp::client::SftpSession as SftpChannel;
use russh_sftp::protocol::FileAttributes;

use oneterm_core::{AppError, TransferEvent};

mod download;
mod pipeline;
pub(super) mod staging;
pub(super) mod upload;

pub(super) const MAX_TRAVERSAL_DEPTH: usize = 64;
pub(super) const MAX_TRAVERSAL_ENTRIES: usize = 100_000;

pub(super) use download::sftp_download;
pub(super) use upload::sftp_upload;

/// Emit `TransferEvent::Cancelled` when `error` is a cancellation, so the UI
/// learns about it even before the result channel settles; other errors pass
/// through untouched.
fn report_cancellation(progress: &Sender<TransferEvent>, error: AppError) -> AppError {
    if matches!(error, AppError::Cancelled) {
        send_progress(progress, TransferEvent::Cancelled);
    }
    error
}

/// Deliver a progress event without blocking the transfer. Progress samples
/// are coalescible: when the UI is behind and the channel is full, dropping
/// this sample is intended (the next one supersedes it); a closed channel
/// means the UI stopped listening, which the result channel reports anyway.
fn send_progress(progress: &Sender<TransferEvent>, event: TransferEvent) {
    if let Err(async_channel::TrySendError::Closed(_)) = progress.try_send(event) {
        log::debug!("sftp transfer: progress consumer is gone");
    }
}

/// Copy the remote permissions and timestamps onto a downloaded local file
/// (SEC-15). Best effort: a failure is logged, the download still succeeded.
async fn apply_remote_metadata_to_local(local: &Path, attrs: &FileAttributes) {
    let local = local.to_path_buf();
    let permissions = attrs.permissions;
    let mtime = attrs.mtime;
    let atime = attrs.atime;
    let result = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).open(&local)?;
        #[cfg(unix)]
        if let Some(mode) = permissions {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(mode & 0o7777))?;
        }
        #[cfg(not(unix))]
        if let Some(mode) = permissions {
            // Only the owner-write bit maps onto Windows: no write bit → read-only.
            let mut permissions = file.metadata()?.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(mode & 0o200 == 0);
            file.set_permissions(permissions)?;
        }
        let mut times = std::fs::FileTimes::new();
        if let Some(mtime) = mtime {
            times = times.set_modified(UNIX_EPOCH + Duration::from_secs(u64::from(mtime)));
        }
        if let Some(atime) = atime {
            times = times.set_accessed(UNIX_EPOCH + Duration::from_secs(u64::from(atime)));
        }
        if mtime.is_some() || atime.is_some() {
            file.set_times(times)?;
        }
        Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            log::warn!("sftp download: could not apply remote permissions/times: {error}")
        }
        Err(join_error) => log::warn!("sftp download: metadata task failed: {join_error}"),
    }
}

/// Copy the local permissions and timestamps onto an uploaded remote file
/// (SEC-15). Best effort: a failure is logged, the upload still succeeded.
async fn apply_local_metadata_to_remote(
    sftp: &SftpChannel,
    remote: &str,
    metadata: &std::fs::Metadata,
) {
    let source = FileAttributes::from(metadata);
    let attrs = FileAttributes {
        // Windows has no Unix mode; do not overwrite the server-side default
        // with the synthetic 0o777 the conversion produces there.
        permissions: if cfg!(unix) {
            source.permissions.map(|mode| mode & 0o7777)
        } else {
            None
        },
        atime: source.atime,
        mtime: source.mtime,
        ..FileAttributes::empty()
    };
    if attrs.permissions.is_none() && attrs.mtime.is_none() {
        return;
    }
    if let Err(error) = sftp.set_metadata(remote, attrs).await {
        log::warn!("sftp upload: could not apply local permissions/times to {remote}: {error}");
    }
}
