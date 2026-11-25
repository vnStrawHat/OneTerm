//! A folder's tree carried either way (specs/oneterm, "M9 in detail"): its folders made, its files
//! copied one by one with the progress over all their bytes.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::files::{Files, join, safe_name};

/// How deep a tree is followed.
const DEEPEST: usize = 32;

impl Files {
    /// Copies a remote folder's tree to `local`, which it makes.
    pub fn download_folder(
        &self,
        remote: &str,
        local: &Path,
        progress: impl Fn(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let mut files: Vec<(String, PathBuf, u64)> = Vec::new();
        let mut folders = vec![(remote.to_string(), local.to_path_buf(), 0)];
        while let Some((dir, here, depth)) = folders.pop() {
            std::fs::create_dir_all(&here).map_err(|e| e.to_string())?;
            for entry in self.list(&dir)? {
                if !safe_name(&entry.name) {
                    return Err(format!("not a name for a local file: {}", entry.name));
                }
                let (there, at) = (join(&dir, &entry.name), here.join(&entry.name));
                match entry.dir {
                    true if depth < DEEPEST => folders.push((there, at, depth + 1)),
                    true => return Err(format!("deeper than {DEEPEST} folders: {there}")),
                    false => files.push((there, at, entry.size)),
                }
            }
        }
        let whole = files.iter().map(|it| it.2).sum::<u64>();
        let mut before = 0;
        for (there, at, size) in files {
            self.download(&there, &at, |d, _| progress(before + d, whole.max(before + d)), cancel)?;
            before += size;
        }
        progress(before, whole.max(before));
        Ok(())
    }

    /// Copies a local folder's tree to `remote`, made where it is not there yet.
    pub fn upload_folder(
        &self,
        local: &Path,
        remote: &str,
        progress: impl Fn(u64, u64),
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let mut files: Vec<(PathBuf, String, u64)> = Vec::new();
        let mut folders = vec![(local.to_path_buf(), remote.to_string(), 0)];
        while let Some((here, there, depth)) = folders.pop() {
            // A folder there already is used.
            if self.mkdir(&there).is_err() && !self.stat(&there).is_ok_and(|it| it.dir) {
                return Err(format!("could not make {there}"));
            }
            for entry in std::fs::read_dir(&here).map_err(|e| e.to_string())?.filter_map(Result::ok) {
                let (name, kind) = (entry.file_name().to_string_lossy().into_owned(), entry.file_type());
                let (at, to) = (entry.path(), join(&there, &name));
                match kind.map_err(|e| e.to_string())? {
                    kind if kind.is_dir() && depth < DEEPEST => folders.push((at, to, depth + 1)),
                    kind if kind.is_dir() => {
                        return Err(format!("deeper than {DEEPEST} folders: {}", at.display()));
                    }
                    kind if kind.is_file() => files.push((at, to, entry.metadata().map_or(0, |it| it.len()))),
                    _ => {}
                }
            }
        }
        let whole = files.iter().map(|it| it.2).sum::<u64>();
        let mut before = 0;
        for (at, to, size) in files {
            self.upload(&at, &to, |d, _| progress(before + d, whole.max(before + d)), cancel)?;
            before += size;
        }
        progress(before, whole.max(before));
        Ok(())
    }
}
