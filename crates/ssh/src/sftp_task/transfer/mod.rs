//! SFTP upload/download orchestration and bounded traversal.

mod download;
pub(super) mod staging;
pub(super) mod upload;

pub(super) const MAX_TRAVERSAL_DEPTH: usize = 64;
pub(super) const MAX_TRAVERSAL_ENTRIES: usize = 100_000;

pub(super) use download::sftp_download;
pub(super) use upload::sftp_upload;
