//! Shared persistence primitives for user-owned JSON and layout state.
//!
//! Domain crates own their serializers and schemas; this module owns the file
//! lifecycle so concurrent writers cannot interleave and completed writes do
//! not expose partially serialized documents.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn lock_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("oneterm-config");
    path.with_file_name(format!(".{name}.lock"))
}

/// An advisory lock held across one complete persistence transaction.
///
/// The lock is backed by the operating system so separate OneTerm processes
/// cannot interleave read-modify-write or atomic replacement operations.
struct InterProcessLock {
    file: File,
}

impl InterProcessLock {
    fn acquire(path: &Path) -> io::Result<Self> {
        let lock_path = lock_path(path);
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock_file(&file)?;
        Ok(Self { file })
    }
}

impl Drop for InterProcessLock {
    fn drop(&mut self) {
        unlock_file(&self.file);
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[cfg(unix)]
fn lock_file(file: &File) -> io::Result<()> {
    const LOCK_EX: i32 = 2;
    let result = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn unlock_file(file: &File) {
    const LOCK_UN: i32 = 8;
    let _ = unsafe { flock(file.as_raw_fd(), LOCK_UN) };
}

#[cfg(windows)]
fn lock_file(file: &File) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LockFileEx};
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn unlock_file(file: &File) {
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    unsafe {
        let _ = UnlockFileEx(
            file.as_raw_handle() as _,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        );
    }
}

#[cfg(not(any(unix, windows)))]
fn lock_file(_file: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn unlock_file(_file: &File) {}

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
/// failed or interrupted migration can be recovered. An operating-system lock
/// serializes writers across processes. The caller is responsible for
/// serialization and schema/version handling.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _lock = InterProcessLock::acquire(path)?;
    atomic_write_unlocked(path, bytes)
}

fn atomic_write_unlocked(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_path(path);
    let write_result = (|| {
        #[cfg(test)]
        maybe_fail(WriteFault::TempCreate)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        #[cfg(test)]
        maybe_fail(WriteFault::TempWrite)?;
        file.write_all(bytes)?;
        #[cfg(test)]
        maybe_fail(WriteFault::Flush)?;
        file.sync_all()?;
        drop(file);

        if path.exists() {
            #[cfg(test)]
            maybe_fail(WriteFault::Backup)?;
            fs::copy(path, backup_path(path))?;
        }

        #[cfg(test)]
        maybe_fail(WriteFault::Replace)?;
        replace_file(&temporary, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum WriteFault {
    TempCreate,
    TempWrite,
    Flush,
    Backup,
    Replace,
}

#[cfg(test)]
thread_local! {
    static WRITE_FAULT: std::cell::Cell<Option<WriteFault>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn inject_write_fault(fault: WriteFault) {
    WRITE_FAULT.with(|current| current.set(Some(fault)));
}

#[cfg(test)]
fn clear_write_fault() {
    WRITE_FAULT.with(|current| current.set(None));
}

#[cfg(test)]
fn maybe_fail(fault: WriteFault) -> io::Result<()> {
    let should_fail = WRITE_FAULT.with(|current| {
        current
            .get()
            .is_some_and(|current| current as u8 == fault as u8)
    });
    if should_fail {
        Err(io::Error::other("injected persistence failure"))
    } else {
        Ok(())
    }
}

/// Serialize a read-modify-write JSON transaction for one file.
pub fn update_json_file(
    path: &Path,
    update: impl FnOnce(&mut serde_json::Value) -> io::Result<()>,
) -> io::Result<()> {
    let _lock = InterProcessLock::acquire(path)?;
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
    let _lock = InterProcessLock::acquire(path)?;
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
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
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
    fn json_updates_are_serialized_across_processes() {
        const WORKER_PATH: &str = "ONETERM_PERSISTENCE_WORKER_PATH";
        if let Ok(path) = std::env::var(WORKER_PATH) {
            update_json_file(Path::new(&path), |document| {
                let count = document["count"].as_u64().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(10));
                document["count"] = serde_json::Value::from(count + 1);
                Ok(())
            })
            .unwrap();
            return;
        }

        let dir = test_dir();
        let path = dir.join("shared-process.json");
        atomic_write(&path, br#"{"count":0}"#).unwrap();
        let mut children = Vec::new();
        for _ in 0..4 {
            children.push(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("json_updates_are_serialized_across_processes")
                    .arg("--nocapture")
                    .env(WORKER_PATH, &path)
                    .spawn()
                    .unwrap(),
            );
        }
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(document["count"], 4);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn injected_write_failures_preserve_the_previous_document() {
        let dir = test_dir();
        let path = dir.join("state.json");
        atomic_write(&path, br#"{"version":1}"#).unwrap();

        for fault in [
            WriteFault::TempCreate,
            WriteFault::TempWrite,
            WriteFault::Flush,
            WriteFault::Backup,
            WriteFault::Replace,
        ] {
            inject_write_fault(fault);
            let result = atomic_write(&path, br#"{"version":2}"#);
            clear_write_fault();
            assert!(result.is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"version":1}"#);
        }
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
