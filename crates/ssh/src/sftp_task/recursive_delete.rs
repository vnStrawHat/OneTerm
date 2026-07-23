//! Bounded recursive deletion for remote SFTP trees.

use std::path::Path;

use russh_sftp::client::SftpSession as SftpChannel;

use oneterm_core::{AppError, Result};

use super::map_sftp_err;
use super::path_policy::validate_remote_entry_name;
use super::transfer::{MAX_TRAVERSAL_DEPTH, MAX_TRAVERSAL_ENTRIES};

/// Remove a file/directory recursively — if a directory, read its contents →
/// remove each entry → remove the dir.
/// Used for `SftpCmd::Rmdir` — supports removing non-empty directories.
pub(super) async fn sftp_remove_recursive(sftp: &SftpChannel, path: &Path) -> Result<()> {
    let root = path.to_string_lossy().replace('\\', "/");
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

        let read_dir = match sftp.read_dir(&current).await {
            Ok(entries) => entries,
            Err(_) => {
                sftp.remove_file(&current).await.map_err(map_sftp_err)?;
                continue;
            }
        };
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
