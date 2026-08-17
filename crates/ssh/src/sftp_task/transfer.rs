//! SFTP upload/download orchestration and bounded traversal.

use async_channel::Sender;

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
        let _ = progress.try_send(TransferEvent::Cancelled);
    }
    error
}
