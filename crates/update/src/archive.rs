use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use oneterm_core::{AppError, Result};

/// Upper bound for the total bytes an update archive may expand to (SEC-20).
///
/// Release packages are tens of megabytes; this only stops a hostile or
/// corrupt archive from filling the disk.
pub const MAX_EXTRACTED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    extract_archive_with_limit(archive_path, destination, MAX_EXTRACTED_BYTES)
}

/// [`extract_archive`] with an explicit expansion budget (tests use a small one).
pub(crate) fn extract_archive_with_limit(
    archive_path: &Path,
    destination: &Path,
    max_extracted_bytes: u64,
) -> Result<()> {
    let name = archive_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let mut budget = ExtractionBudget::new(max_extracted_bytes);
    if name.ends_with(".zip") {
        extract_zip(archive_path, destination, &mut budget)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive_path, destination, &mut budget)
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

/// Remaining bytes an extraction may still write.
struct ExtractionBudget {
    remaining: u64,
    limit: u64,
}

impl ExtractionBudget {
    fn new(limit: u64) -> Self {
        Self {
            remaining: limit,
            limit,
        }
    }

    fn consume(&mut self, bytes: u64) -> Result<()> {
        match self.remaining.checked_sub(bytes) {
            Some(remaining) => {
                self.remaining = remaining;
                Ok(())
            }
            None => Err(self.exceeded()),
        }
    }

    fn exceeded(&self) -> AppError {
        AppError::msg(format!(
            "update archive expands beyond the {} byte limit",
            self.limit
        ))
    }

    /// Stream `reader` into `writer`, charging every byte to the budget so a
    /// header that understates the entry size cannot bypass the limit.
    fn copy(&mut self, reader: &mut impl Read, writer: &mut impl Write) -> Result<()> {
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                return Ok(());
            }
            self.consume(read as u64)?;
            writer.write_all(&buffer[..read])?;
        }
    }
}

fn extract_zip(
    archive_path: &Path,
    destination: &Path,
    budget: &mut ExtractionBudget,
) -> Result<()> {
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
        // Same guard as the tar path: a symlink could point the later package
        // validation or install copy outside the staging directory (SEC-21).
        if entry.is_symlink() {
            return Err(AppError::msg(format!(
                "update archive contains a symlink, which is not allowed: {}",
                entry.name()
            )));
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Cheap early rejection from the declared size; the streaming copy
            // below charges the bytes actually produced.
            if entry.size() > budget.remaining {
                return Err(budget.exceeded());
            }
            let mut output_file = File::create(&output)?;
            budget.copy(&mut entry, &mut output_file)?;
        }
    }
    Ok(())
}

fn extract_tar_gz(
    archive_path: &Path,
    destination: &Path,
    budget: &mut ExtractionBudget,
) -> Result<()> {
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
        if !(kind.is_file() || kind.is_dir()) {
            return Err(AppError::msg(format!(
                "update archive contains an unsupported entry type: {kind:?}"
            )));
        }
        let relative = entry.path()?.into_owned();
        reject_unsafe_path(&relative)?;
        // The tar reader yields exactly `size` bytes per entry, so the header
        // is authoritative for the expansion budget.
        budget.consume(entry.header().size()?)?;
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
#[path = "archive_tests.rs"]
mod archive_tests;
