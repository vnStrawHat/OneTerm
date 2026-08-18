//! Bounded recursive deletion for remote SFTP trees.

use russh_sftp::client::SftpSession as SftpChannel;

use oneterm_core::{AppError, RemotePath, Result};

use super::map_sftp_err;
use super::path_policy::validate_remote_entry_name;
use super::transfer::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES};

/// Remove `path` and everything below it — the implementation of
/// `SftpBackend::remove_dir_all`.
///
/// The root is inspected with `lstat` first: a symlink (even one pointing at a
/// directory) or a non-directory is unlinked with `remove_file`, so a symlinked
/// root never causes the link *target* to be emptied. Below the root, symlinks
/// are likewise unlinked rather than descended into. Directory listing errors
/// (permission denied, transient failures) are propagated instead of being
/// mistaken for "not a directory".
pub(super) async fn sftp_remove_recursive(sftp: &SftpChannel, path: &RemotePath) -> Result<()> {
    let root = path.as_str().to_string();
    let root_attrs = sftp.symlink_metadata(&root).await.map_err(map_sftp_err)?;
    if root_attrs.is_symlink() || !root_attrs.is_dir() {
        return sftp.remove_file(&root).await.map_err(map_sftp_err);
    }

    let mut pending = vec![(root, false, 0usize)];
    let mut visited = 0usize;

    while let Some((current, expanded, depth)) = pending.pop() {
        if depth > MAX_TRAVERSAL_DEPTH {
            return Err(AppError::msg(
                "remote deletion exceeded traversal depth limit",
            ));
        }
        if expanded {
            sftp.remove_dir(&current).await.map_err(map_sftp_err)?;
            continue;
        }

        let read_dir = sftp.read_dir(&current).await.map_err(map_sftp_err)?;
        pending.push((current.clone(), true, depth));

        for entry in read_dir {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            validate_remote_entry_name(&name)?;
            visited += 1;
            if visited > MAX_TRAVERSAL_ENTRIES {
                return Err(AppError::msg(
                    "remote deletion exceeded traversal entry limit",
                ));
            }
            let child = format!("{}/{}", current.trim_end_matches('/'), name);
            let metadata = entry.metadata();
            if metadata.is_dir() && !metadata.is_symlink() {
                pending.push((child, false, depth + 1));
            } else {
                sftp.remove_file(&child).await.map_err(map_sftp_err)?;
            }
        }
    }
    Ok(())
}
