//! SFTP tokio task — handles `SftpCmd` from the UI, calls the `russh_sftp` API.
//!
//! Runs alongside `ssh_main_task` on the same tokio runtime.
//! The two channels (shell + sftp) share one TCP connection, multiplexed by russh.
//!
//! Upload/download are spawned as separate tokio tasks — the main loop stays
//! responsive to receive `SftpCmd::Cancel` and signal the `CancellationToken`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use russh_sftp::client::SftpSession as SftpChannel;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use oneterm_core::{AppError, SftpStatus};

use crate::sftp::{SftpCmd, SftpEvent};

mod metadata;
mod path_policy;
mod recursive_delete;
mod transfer;
mod transfer_registry;

use metadata::{load_uid_gid_lookup, sftp_read_dir, sftp_stat};
use recursive_delete::sftp_remove_recursive;
use transfer::{sftp_download, sftp_upload};
use transfer_registry::{ActiveTransferGuard, ActiveTransfers};

pub(crate) use path_policy::{
    create_safe_parent_dirs, safe_local_child, validate_remote_entry_name,
};

/// Tokio task that handles SFTP commands.
///
/// Runs alongside `ssh_main_task` on the same tokio runtime.
/// Receives `SftpCmd` via `cmd_rx`, sends `SftpEvent` via `event_tx`.
///
/// Upload/download are spawned as separate tokio tasks — the main loop stays
/// responsive to receive `SftpCmd::Cancel` and signal the `CancellationToken`.
///
/// `shutdown` is cancelled by `ssh_main_task` when the SSH connection ends, so
/// the SFTP task (and `alive()`) follow the connection lifetime (ARCH-28).
pub(crate) async fn sftp_task(
    sftp: SftpChannel,
    cmd_rx: Receiver<SftpCmd>,
    event_tx: Sender<SftpEvent>,
    alive: std::sync::Arc<std::sync::Mutex<bool>>,
    shutdown: CancellationToken,
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
    let cancels: ActiveTransfers = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut background_tasks = JoinSet::new();

    loop {
        let command = tokio::select! {
            command = cmd_rx.recv() => Some(command),
            completed = background_tasks.join_next(), if !background_tasks.is_empty() => {
                if let Some(Err(error)) = completed {
                    log::error!("sftp_task: background task failed: {error}");
                }
                None
            }
            _ = shutdown.cancelled() => {
                log::info!("sftp_task: SSH connection ended — shutting down");
                for cancellation in cancels.lock().unwrap().values() {
                    cancellation.cancel();
                }
                break;
            }
        };
        let Some(command) = command else {
            continue;
        };
        match command {
            Ok(SftpCmd::ReadDir { path, reply }) => {
                log::debug!("sftp_task: ReadDir path=\"{path}\"");
                let result = sftp_read_dir(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Stat { path, reply }) => {
                log::debug!("sftp_task: Stat path=\"{path}\"");
                let result = sftp_stat(&sftp, &path, &lookup).await;
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Rename { from, to, reply }) => {
                log::debug!("sftp_task: Rename from=\"{from}\" to=\"{to}\"");
                let result = sftp
                    .rename(from.as_str(), to.as_str())
                    .await
                    .map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Remove { path, reply }) => {
                log::debug!("sftp_task: Remove path=\"{path}\"");
                let result = sftp.remove_file(path.as_str()).await.map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::RemoveDirAll { path, reply }) => {
                log::debug!("sftp_task: RemoveDirAll path=\"{path}\"");
                let sftp = Arc::clone(&sftp);
                background_tasks.spawn(async move {
                    let result = sftp_remove_recursive(&sftp, &path).await;
                    let _ = reply.send(result);
                });
            }
            Ok(SftpCmd::Mkdir { path, reply }) => {
                log::debug!("sftp_task: Mkdir path=\"{path}\"");
                let result = sftp.create_dir(path.as_str()).await.map_err(map_sftp_err);
                let _ = reply.send(result);
            }
            Ok(SftpCmd::Upload {
                transfer_id,
                local,
                remote,
                events,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Upload #{transfer_id} local=\"{}\" remote=\"{remote}\"",
                    local.display()
                );
                let cancel = CancellationToken::new();
                {
                    let mut active = cancels.lock().unwrap();
                    if active.contains_key(&transfer_id) {
                        let _ = reply.try_send(Err(AppError::msg(format!(
                            "duplicate active transfer id: {transfer_id}"
                        ))));
                        continue;
                    }
                    active.insert(transfer_id, cancel.clone());
                }
                let sftp = Arc::clone(&sftp);
                let cancels = Arc::clone(&cancels);
                background_tasks.spawn(async move {
                    let _cleanup = ActiveTransferGuard::new(cancels, transfer_id);
                    let result = sftp_upload(&sftp, &local, &remote, &events, &cancel).await;
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
                events,
                reply,
            }) => {
                log::info!(
                    "sftp_task: Download #{transfer_id} remote=\"{remote}\" local=\"{}\"",
                    local.display()
                );
                let cancel = CancellationToken::new();
                {
                    let mut active = cancels.lock().unwrap();
                    if active.contains_key(&transfer_id) {
                        let _ = reply.try_send(Err(AppError::msg(format!(
                            "duplicate active transfer id: {transfer_id}"
                        ))));
                        continue;
                    }
                    active.insert(transfer_id, cancel.clone());
                }
                let sftp = Arc::clone(&sftp);
                let cancels = Arc::clone(&cancels);
                background_tasks.spawn(async move {
                    let _cleanup = ActiveTransferGuard::new(cancels, transfer_id);
                    let result = sftp_download(&sftp, &remote, &local, &events, &cancel).await;
                    log::info!(
                        "sftp_task: Download #{transfer_id} finished: {}",
                        if result.is_ok() { "OK" } else { "error" }
                    );
                    let _ = reply.try_send(result);
                });
            }
            Ok(SftpCmd::Cancel { transfer_id }) => {
                log::info!("sftp_task: Cancel transfer #{transfer_id}");
                let cancel = cancels.lock().unwrap().get(&transfer_id).cloned();
                if let Some(cancel) = cancel {
                    cancel.cancel();
                    log::info!("sftp_task: Cancel #{transfer_id} — token signalled");
                } else {
                    log::warn!("sftp_task: Cancel #{transfer_id} — not found (already finished?)");
                }
            }
            Ok(SftpCmd::Close) => {
                log::info!("sftp_task: close requested");
                for cancellation in cancels.lock().unwrap().values() {
                    cancellation.cancel();
                }
                break;
            }
            Err(_) => {
                log::info!("sftp_task: cmd_rx closed — session dropped");
                for cancellation in cancels.lock().unwrap().values() {
                    cancellation.cancel();
                }
                break;
            }
        }
    }

    const BACKGROUND_TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
    let drain_tasks = async {
        while let Some(result) = background_tasks.join_next().await {
            if let Err(error) = result {
                log::error!("sftp_task: background task failed during shutdown: {error}");
            }
        }
    };
    if tokio::time::timeout(BACKGROUND_TASK_SHUTDOWN_TIMEOUT, drain_tasks)
        .await
        .is_err()
    {
        log::warn!("sftp_task: background-task shutdown timed out; aborting remaining tasks");
        background_tasks.abort_all();
        while let Some(result) = background_tasks.join_next().await {
            if let Err(error) = result
                && !error.is_cancelled()
            {
                log::error!("sftp_task: aborted background task failed: {error}");
            }
        }
    }
    cancels.lock().unwrap().clear();

    {
        let mut a = alive.lock().unwrap();
        *a = false;
    }
    let _ = event_tx.try_send(SftpEvent::Closed);
    log::info!("sftp_task: exiting");
}

// ── Helpers ──────────────────────────────────────────────────

/// Convert a russh-sftp error to `AppError`.
///
/// A server `Status` reply keeps its code as [`SftpStatus`] so the UI can tell
/// permission-denied from not-found; transport-level failures (timeout, I/O,
/// protocol violations) stay opaque messages.
pub(crate) fn map_sftp_err(e: russh_sftp::client::error::Error) -> AppError {
    use russh_sftp::client::error::Error;
    use russh_sftp::protocol::StatusCode;

    match e {
        Error::Status(status) => {
            let code = match status.status_code {
                StatusCode::Ok => SftpStatus::Other(0),
                StatusCode::Eof => SftpStatus::Eof,
                StatusCode::NoSuchFile => SftpStatus::NoSuchFile,
                StatusCode::PermissionDenied => SftpStatus::PermissionDenied,
                StatusCode::Failure => SftpStatus::Failure,
                StatusCode::BadMessage => SftpStatus::BadMessage,
                StatusCode::NoConnection | StatusCode::ConnectionLost => SftpStatus::ConnectionLost,
                StatusCode::OpUnsupported => SftpStatus::OpUnsupported,
            };
            AppError::Sftp {
                status: code,
                message: status.error_message,
            }
        }
        other => AppError::msg(other.to_string()),
    }
}

#[cfg(test)]
mod sftp_task_tests;
