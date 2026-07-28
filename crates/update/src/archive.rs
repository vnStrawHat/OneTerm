use std::fs::File;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use oneterm_core::{AppError, Result};

pub fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name.ends_with(".zip") {
        extract_zip(archive_path, destination)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive_path, destination)
    } else {
        Err(AppError::msg(format!(
            "unsupported update archive format: {name}"
        )))
    }
}

pub fn validate_staged_package(extract_dir: &Path, target_triple: &str) -> Result<PathBuf> {
    let required = RequiredEntry::for_target(target_triple);
    let candidates = package_dir_candidates(extract_dir, target_triple)?;
    for candidate in &candidates {
        if required.exists(candidate) {
            return Ok(candidate.clone());
        }
    }

    let fallback = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| extract_dir.to_path_buf());
    required.require(&fallback)?;
    Ok(fallback)
}

#[derive(Clone, Copy)]
enum RequiredEntryKind {
    File,
    Directory,
}

#[derive(Clone, Copy)]
struct RequiredEntry {
    relative_path: &'static str,
    kind: RequiredEntryKind,
}

impl RequiredEntry {
    fn for_target(target_triple: &str) -> Self {
        if target_triple.contains("windows") {
            Self {
                relative_path: "oneterm.exe",
                kind: RequiredEntryKind::File,
            }
        } else if target_triple.contains("apple-darwin") {
            Self {
                relative_path: "OneTerm.app",
                kind: RequiredEntryKind::Directory,
            }
        } else {
            Self {
                relative_path: "oneterm",
                kind: RequiredEntryKind::File,
            }
        }
    }

    fn path_in(self, package_dir: &Path) -> PathBuf {
        package_dir.join(self.relative_path)
    }

    fn exists(self, package_dir: &Path) -> bool {
        let path = self.path_in(package_dir);
        match self.kind {
            RequiredEntryKind::File => path.is_file(),
            RequiredEntryKind::Directory => path.is_dir(),
        }
    }

    fn require(self, package_dir: &Path) -> Result<()> {
        let path = self.path_in(package_dir);
        match self.kind {
            RequiredEntryKind::File => require_file(&path),
            RequiredEntryKind::Directory => require_dir(&path),
        }
    }
}

fn package_dir_candidates(extract_dir: &Path, target_triple: &str) -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    collect_matching_package_dirs(extract_dir, target_triple, 0, &mut candidates)?;
    push_unique(&mut candidates, extract_dir.to_path_buf());
    Ok(candidates)
}

fn collect_matching_package_dirs(
    dir: &Path,
    target_triple: &str,
    depth: usize,
    candidates: &mut Vec<PathBuf>,
) -> Result<()> {
    if depth > 2 {
        return Ok(());
    }

    let mut child_dirs = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            child_dirs.push(entry.path());
        }
    }
    child_dirs.sort();

    for child in child_dirs {
        if is_release_package_dir(&child, target_triple) {
            push_unique(candidates, child.clone());
        }
        if depth < 2 {
            collect_matching_package_dirs(&child, target_triple, depth + 1, candidates)?;
        }
    }

    Ok(())
}

fn is_release_package_dir(path: &Path, target_triple: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| is_release_package_name(name, target_triple))
}

fn is_release_package_name(name: &str, target_triple: &str) -> bool {
    let unversioned = format!("oneterm-{target_triple}");
    let versioned_suffix = format!("-{target_triple}");
    name == unversioned || (name.starts_with("oneterm-") && name.ends_with(&versioned_suffix))
}

fn push_unique(values: &mut Vec<PathBuf>, value: PathBuf) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| AppError::msg(format!("invalid zip archive: {error}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| AppError::msg(format!("invalid zip entry: {error}")))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(AppError::msg(format!(
                "unsafe path in zip archive: {}",
                entry.name()
            )));
        };
        reject_unsafe_path(&relative)?;
        let output = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut output_file = File::create(&output)?;
            std::io::copy(&mut entry, &mut output_file)?;
        }
    }
    Ok(())
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(AppError::msg(
                "update archive contains links, which are not allowed",
            ));
        }
        let relative = entry.path()?.into_owned();
        reject_unsafe_path(&relative)?;
        let output = destination.join(relative);
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&output)?;
    }
    Ok(())
}

fn reject_unsafe_path(path: &Path) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(AppError::msg(format!(
            "update archive contains unsafe path: {}",
            path.display()
        )));
    }
    Ok(())
}

fn require_file(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "staged update is missing required file: {}",
            path.display()
        )))
    }
}

fn require_dir(path: &Path) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(AppError::msg(format!(
            "staged update is missing required directory: {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_paths() {
        assert!(reject_unsafe_path(Path::new("../outside")).is_err());
    }

    #[test]
    fn accepts_relative_paths() {
        reject_unsafe_path(Path::new("oneterm-x86_64-unknown-linux-gnu/oneterm")).unwrap();
    }

    #[test]
    fn accepts_versioned_windows_package_dir() {
        let dir = test_dir("versioned-windows");
        let package_dir = dir.join("oneterm-0.3.1-x86_64-pc-windows-msvc");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(package_dir.join("oneterm.exe"), b"binary").unwrap();
        let validated = validate_staged_package(&dir, "x86_64-pc-windows-msvc").unwrap();
        assert_eq!(validated, package_dir);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn accepts_versioned_windows_package_dir_inside_dist() {
        let dir = test_dir("versioned-windows-dist");
        let package_dir = dir
            .join("dist")
            .join("oneterm-0.3.1-x86_64-pc-windows-msvc");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(package_dir.join("oneterm.exe"), b"binary").unwrap();
        let validated = validate_staged_package(&dir, "x86_64-pc-windows-msvc").unwrap();
        assert_eq!(validated, package_dir);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "oneterm-archive-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }
}
