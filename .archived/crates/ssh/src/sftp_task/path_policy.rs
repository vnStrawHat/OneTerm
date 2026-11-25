//! Local destination policy for untrusted remote SFTP names.

use std::path::{Path, PathBuf};

use oneterm_core::{AppError, Result};

/// Validate one remote directory entry before using it as a local path component.
///
/// Remote names are treated as untrusted input. They must remain one normal
/// component on every supported client platform; separators, prefixes, reserved
/// Windows names, and trailing dot/space forms are rejected.
pub(crate) fn validate_remote_entry_name(name: &str) -> Result<()> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(AppError::msg("unsafe remote filename"));
    }
    if name
        .chars()
        .any(|ch| ch == '/' || ch == '\\' || ch == ':' || ch == '\0')
    {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    if !matches!(
        Path::new(name).components().next(),
        Some(std::path::Component::Normal(component))
            if component == std::ffi::OsStr::new(name)
    ) {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }

    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Err(AppError::msg(format!("unsafe remote filename: {name:?}")));
    }
    Ok(())
}

/// Join a validated remote name to a local root without allowing traversal.
pub(crate) fn safe_local_child(root: &Path, name: &str) -> Result<PathBuf> {
    validate_remote_entry_name(name)?;
    let child = root.join(name);
    if !child.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    Ok(child)
}

/// Walk `target` one component at a time below `root`, rejecting non-normal
/// components and pre-existing symlinks.
///
/// With `create_dirs` the walk also requires every existing component to be a
/// directory, creates the missing ones, and re-checks after each level that the
/// canonical path is still below `root` (a component swapped for a link while
/// the walk runs cannot escape).
async fn walk_below_root(root: &Path, target: &Path, create_dirs: bool) -> Result<()> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| AppError::msg("local download path escaped destination"))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(AppError::msg("unsafe local download component"));
        };
        current.push(component);
        match tokio::fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::msg(format!(
                    "refusing to traverse local symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if create_dirs && !metadata.is_dir() => {
                return Err(AppError::msg(format!(
                    "local download parent is not a directory: {}",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create_dirs {
                    tokio::fs::create_dir(&current)
                        .await
                        .map_err(|e| AppError::msg(format!("create local directory: {e}")))?;
                }
            }
            Err(error) => {
                return Err(AppError::msg(format!(
                    "inspect local download path {}: {error}",
                    current.display()
                )));
            }
        }
        if create_dirs {
            let canonical = tokio::fs::canonicalize(&current)
                .await
                .map_err(|e| AppError::msg(format!("canonicalize download directory: {e}")))?;
            if !canonical.starts_with(root) {
                return Err(AppError::msg("local download path escaped destination"));
            }
        }
    }
    Ok(())
}

/// Reject existing symlinks and verify that a local destination remains below
/// the selected root before writing it.
async fn ensure_local_destination(root: &Path, candidate: &Path) -> Result<()> {
    if !candidate.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    walk_below_root(root, candidate, false).await?;

    let parent = candidate.parent().unwrap_or(root);
    let canonical_parent = tokio::fs::canonicalize(parent)
        .await
        .map_err(|e| AppError::msg(format!("canonicalize download parent: {e}")))?;
    if !canonical_parent.starts_with(root) {
        return Err(AppError::msg("local download path escaped destination"));
    }
    Ok(())
}

/// Create missing destination directories one component at a time while
/// rejecting pre-existing symlinks and non-directory components.
pub(crate) async fn create_safe_parent_dirs(root: &Path, candidate: &Path) -> Result<()> {
    walk_below_root(root, candidate.parent().unwrap_or(root), true).await?;
    ensure_local_destination(root, candidate).await
}
