//! Shared persistence primitives for user-owned JSON and layout state.
//!
//! Domain crates own their serializers and schemas; this module owns the file
//! lifecycle so concurrent writers cannot interleave and completed writes do
//! not expose partially serialized documents.

use std::collections::HashMap;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static FILE_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn file_lock(path: &Path) -> Arc<Mutex<()>> {
    let locks = FILE_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("oneterm-config");
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id()))
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

/// Write bytes through a same-directory temporary file and final rename.
///
/// Existing content is copied to a sibling `.bak` file before replacement so a
/// failed or interrupted migration can be recovered. Writers for the same path
/// are serialized in-process. The caller is responsible for serialization and
/// schema/version handling.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let lock = file_lock(path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    atomic_write_unlocked(path, bytes)
}

fn atomic_write_unlocked(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_path(path);
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);

        if path.exists() {
            fs::copy(path, backup_path(path))?;
        }

        replace_file(&temporary, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

/// Serialize a read-modify-write JSON transaction for one file.
pub fn update_json_file(
    path: &Path,
    update: impl FnOnce(&mut serde_json::Value) -> io::Result<()>,
) -> io::Result<()> {
    let lock = file_lock(path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut value = match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => serde_json::json!({}),
        Err(error) => return Err(error),
    };
    update(&mut value)?;
    let bytes = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
    atomic_write_unlocked(path, &bytes)
}

#[cfg(unix)]
fn replace_file(temporary: &Path, target: &Path) -> io::Result<()> {
    fs::rename(temporary, target)?;
    if let Some(parent) = target.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(temporary: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};

    if !target.exists() {
        return fs::rename(temporary, target);
    }

    let target_wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let temporary_wide: Vec<u16> = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            temporary_wide.as_ptr(),
            null(),
            REPLACEFILE_WRITE_THROUGH,
            null(),
            null(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn replace_file(temporary: &Path, target: &Path) -> io::Result<()> {
    fs::rename(temporary, target)
}

/// Move an unreadable persisted document aside for diagnosis and recovery.
///
/// The original path becomes available for a default configuration to be
/// written. The quarantine name is unique within the process.
pub fn quarantine_file(path: &Path) -> io::Result<Option<PathBuf>> {
    let lock = file_lock(path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !path.exists() {
        return Ok(None);
    }
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("oneterm-config.json");
    let quarantined = path.with_file_name(format!(".{name}.invalid-{sequence}"));
    fs::rename(path, &quarantined)?;
    Ok(Some(quarantined))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "oneterm-persistence-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn atomic_write_replaces_complete_content_and_keeps_backup() {
        let dir = test_dir();
        let path = dir.join("state.json");
        atomic_write(&path, br#"{"version":1}"#).unwrap();
        atomic_write(&path, br#"{"version":2}"#).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"version":2}"#);
        assert_eq!(
            fs::read_to_string(dir.join("state.bak")).unwrap(),
            r#"{"version":1}"#
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn json_updates_serialize_the_read_modify_write_transaction() {
        let dir = test_dir();
        let path = dir.join("shared.json");
        atomic_write(&path, br#"{"count":0}"#).unwrap();

        let mut writers = Vec::new();
        for _ in 0..8 {
            let path = path.clone();
            writers.push(std::thread::spawn(move || {
                update_json_file(&path, |document| {
                    let count = document["count"].as_u64().unwrap();
                    document["count"] = serde_json::Value::from(count + 1);
                    Ok(())
                })
                .unwrap();
            }));
        }
        for writer in writers {
            writer.join().unwrap();
        }

        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(document["count"], 8);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn quarantine_moves_invalid_content_without_overwriting_it() {
        let dir = test_dir();
        let path = dir.join("state.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, b"invalid").unwrap();

        let quarantined = quarantine_file(&path).unwrap().unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(quarantined).unwrap(), b"invalid");
        let _ = fs::remove_dir_all(dir);
    }
}
