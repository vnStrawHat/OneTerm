//! Shared persistence primitives for user-owned JSON and layout state.
//!
//! Domain crates own their serializers and schemas; this module owns the file
//! lifecycle so concurrent writers cannot interleave and completed writes do
//! not expose partially serialized documents.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
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
        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for InterProcessLock {
    fn drop(&mut self) {
        // Releasing a held lock cannot fail in a way the caller can act on.
        let _ = self.file.unlock();
    }
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
/// failed or interrupted migration can be recovered. An operating-system lock
/// serializes writers across processes. The caller is responsible for
/// serialization and schema/version handling.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let _lock = InterProcessLock::acquire(path)?;
    atomic_write_unlocked(path, bytes, Sensitivity::Ordinary)
}

/// [`atomic_write`] for a document that holds sealed secrets (`DEC-0021`;
/// `ssh_session.json` since `US-0158`).
///
/// On Unix the document and its backup are owner-only (`0600`) on **every**
/// write. Under the document's lock, `retires_secret` is shown the document
/// being replaced, when there is one: when it answers `true` (the new bytes
/// drop or replace a sealed value) `.bak` becomes a copy of the new document,
/// so the retired value does not outlive this write; otherwise `.bak` keeps
/// the previous generation like any other document's.
pub fn atomic_write_sealed(
    path: &Path,
    bytes: &[u8],
    retires_secret: impl FnOnce(&[u8]) -> bool,
) -> io::Result<()> {
    let _lock = InterProcessLock::acquire(path)?;
    let sensitivity = match fs::read(path) {
        Ok(previous) if retires_secret(&previous) => Sensitivity::Secret,
        Ok(_) => Sensitivity::Private,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Sensitivity::Private,
        Err(error) => return Err(error),
    };
    atomic_write_unlocked(path, bytes, sensitivity)
}

/// Whether a document holds secret material (`DEC-0021`), which changes how
/// its files are written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sensitivity {
    /// `.bak` keeps the previous generation; files get the process umask.
    Ordinary,
    /// Holds sealed secrets but this write retires none: `.bak` keeps the
    /// previous generation; on Unix every file the write creates is
    /// owner-only (`0600`).
    Private,
    /// `.bak` becomes a copy of the new document, so an old wrapped key or
    /// ciphertext does not outlive the write that replaced it; on Unix every
    /// file the write creates is owner-only (`0600`).
    Secret,
}

fn atomic_write_unlocked(path: &Path, bytes: &[u8], sensitivity: Sensitivity) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = temporary_path(path);
    let write_result = (|| {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        if sensitivity != Sensitivity::Ordinary {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);

        // `fs::copy` carries the source's permission bits, so a scrubbed
        // backup is owner-only too.
        match sensitivity {
            Sensitivity::Ordinary | Sensitivity::Private if path.exists() => {
                fs::copy(path, backup_path(path))?;
                // The previous generation may predate the owner-only rule.
                #[cfg(unix)]
                if sensitivity == Sensitivity::Private {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(backup_path(path), fs::Permissions::from_mode(0o600))?;
                }
            }
            Sensitivity::Ordinary | Sensitivity::Private => {}
            Sensitivity::Secret => {
                fs::copy(&temporary, backup_path(path))?;
            }
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
    update_json_file_as(path, update, Sensitivity::Ordinary)
}

/// [`update_json_file`] for a document that holds secret material
/// (`DEC-0021`; today only `vault.json`).
///
/// The sibling `.bak` is replaced by a copy of the **new** document instead of
/// the previous generation, so a write that retires a secret (a password
/// change rewrapping the data key) leaves no copy of it behind. On Unix the
/// document and its backup are created owner-only (`0600`); on Windows the
/// profile ACL applies.
pub fn update_secret_json_file(
    path: &Path,
    update: impl FnOnce(&mut serde_json::Value) -> io::Result<()>,
) -> io::Result<()> {
    update_json_file_as(path, update, Sensitivity::Secret)
}

fn update_json_file_as(
    path: &Path,
    update: impl FnOnce(&mut serde_json::Value) -> io::Result<()>,
    sensitivity: Sensitivity,
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
    atomic_write_unlocked(path, &bytes, sensitivity)
}

/// Remove a document and its `.bak` under the document's inter-process lock.
///
/// Either file being absent already is not an error. The lock file itself
/// stays, like every lock file (`docs/agents/persistence.md`).
pub fn remove_document(path: &Path) -> io::Result<()> {
    // M4 (`DEC-0019`), guarded here like `quarantine_file`: a removal is a
    // write, and this function is the deepest point every caller passes
    // through. Unlike a quarantine it is refused loudly, so a caller never
    // believes a document is gone when it is not.
    if crate::config::elevation::is_restricted() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "an elevated OneTerm window writes no configuration",
        ));
    }
    let _lock = InterProcessLock::acquire(path)?;
    for file in [path.to_path_buf(), backup_path(path)] {
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Replace `target` with `temporary`. `fs::rename` overwrites an existing
/// destination on both Unix and Windows; Unix additionally needs the parent
/// directory synced so the new name survives a crash.
fn replace_file(temporary: &Path, target: &Path) -> io::Result<()> {
    fs::rename(temporary, target)?;
    #[cfg(unix)]
    if let Some(parent) = target.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

/// Move an unreadable persisted document aside for diagnosis and recovery.
///
/// The original path becomes available for a default configuration to be
/// written. The quarantine name carries the process id, a timestamp and a
/// process-local sequence, so a quarantine from an earlier run (or another
/// process) is never overwritten by this one; an unexpected collision is
/// skipped by advancing the sequence instead of replacing the older file.
pub fn quarantine_file(path: &Path) -> io::Result<Option<PathBuf>> {
    // M4 (`DEC-0019`, `IN-0043`): read-only means read-only. A quarantine is a
    // **rename** of the user's file, and the M4 guards sit on the write entry
    // points, which this is not one of. An elevated window that met a corrupt
    // document would otherwise modify the config directory — under
    // over-the-shoulder elevation, another account's — which is the exact trace
    // M4 says is never left. It still starts on the defaults and still says so
    // once; it just leaves the file where it found it, for the ordinary window
    // to quarantine. One guard here covers every caller
    // (`ui_config.rs`, `terminal_config/document.rs`, `dock_persistence`).
    if crate::config::elevation::is_restricted() {
        log::info!(
            "elevated window: not quarantining {}; using the defaults and leaving the file untouched",
            path.display()
        );
        return Ok(None);
    }
    let _lock = InterProcessLock::acquire(path)?;
    if !path.exists() {
        return Ok(None);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("oneterm-config.json");
    let pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let quarantined =
            path.with_file_name(format!(".{name}.invalid-{pid}-{timestamp}-{sequence}"));
        // `create_new` reserves the name without replacing an existing file;
        // the rename below then replaces only our own placeholder.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&quarantined)
        {
            Ok(_placeholder) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
        fs::rename(path, &quarantined)?;
        return Ok(Some(quarantined));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Distinct per call so concurrently running tests never share a directory,
    // even when the wall clock is too coarse to separate two calls (as on
    // macOS). The atomic counter guarantees uniqueness within the process; the
    // pid and timestamp keep names unique across processes and test runs.
    static TEST_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    /// Removes the fixture directory on drop, including when an assertion
    /// panics (`Drop` runs during unwind): a manual `fs::remove_dir_all` as
    /// a test's last statement never runs if an earlier assertion in the
    /// same test panics (`BUG-0083`, same latent-leak shape
    /// `crates/app/src/crash_report.rs`'s `TemporaryDirectory` fixed for
    /// `BUG-0082`).
    struct TemporaryDirectory(PathBuf);

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    impl std::ops::Deref for TemporaryDirectory {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    fn test_dir() -> TemporaryDirectory {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEST_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        TemporaryDirectory(std::env::temp_dir().join(format!(
            "oneterm-persistence-{}-{nonce}-{sequence}",
            std::process::id()
        )))
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
        assert_eq!(fs::read_dir(&*dir).unwrap().count(), 3);
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
    }

    /// A failed write leaves the previous document intact and removes the
    /// temporary file. A directory occupying the `.bak` name makes the backup
    /// copy fail on every platform.
    #[test]
    fn failed_writes_preserve_the_previous_document() {
        let dir = test_dir();
        let path = dir.join("state.json");
        atomic_write(&path, br#"{"version":1}"#).unwrap();
        fs::create_dir(backup_path(&path)).unwrap();

        assert!(atomic_write(&path, br#"{"version":2}"#).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"version":1}"#);
        // No temporary file was left behind (state.json, state.bak/, .lock).
        assert_eq!(fs::read_dir(&*dir).unwrap().count(), 3);
    }

    #[test]
    fn secret_updates_leave_a_backup_equal_to_the_new_document() {
        let dir = test_dir();
        let path = dir.join("secret.json");
        for generation in [1, 2] {
            update_secret_json_file(&path, |document| {
                document["generation"] = serde_json::Value::from(generation);
                Ok(())
            })
            .unwrap();
        }

        let current = fs::read(&path).unwrap();
        let document: serde_json::Value = serde_json::from_slice(&current).unwrap();
        assert_eq!(document["generation"], 2);
        assert_eq!(fs::read(backup_path(&path)).unwrap(), current);
    }

    #[cfg(unix)]
    #[test]
    fn secret_documents_and_their_backups_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = test_dir();
        let path = dir.join("secret.json");
        update_secret_json_file(&path, |document| {
            document["generation"] = serde_json::Value::from(1);
            Ok(())
        })
        .unwrap();

        for file in [path.clone(), backup_path(&path)] {
            let mode = fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{}", file.display());
        }
    }

    /// `US-0158`: a sealed document keeps the previous generation in `.bak`
    /// unless the write retires a secret, and the decision sees the document
    /// being replaced.
    #[test]
    fn sealed_writes_scrub_the_backup_only_when_they_retire_a_secret() {
        let dir = test_dir();
        let path = dir.join("sealed.json");
        let never = |_: &[u8]| -> bool { unreachable!("no previous document") };
        atomic_write_sealed(&path, b"one", never).unwrap();
        assert!(!backup_path(&path).exists());

        atomic_write_sealed(&path, b"two", |previous| {
            assert_eq!(previous, b"one");
            false
        })
        .unwrap();
        assert_eq!(fs::read(backup_path(&path)).unwrap(), b"one");

        atomic_write_sealed(&path, b"three", |_| true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"three");
        assert_eq!(fs::read(backup_path(&path)).unwrap(), b"three");
    }

    #[cfg(unix)]
    #[test]
    fn sealed_documents_and_their_backups_are_owner_only_on_every_write() {
        use std::os::unix::fs::PermissionsExt;

        let dir = test_dir();
        let path = dir.join("sealed.json");
        atomic_write(&path, b"umask").unwrap();
        atomic_write_sealed(&path, b"kept", |_| false).unwrap();

        for file in [path.clone(), backup_path(&path)] {
            let mode = fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "{}", file.display());
        }
    }

    #[test]
    fn remove_document_removes_the_file_and_its_backup_and_tolerates_absence() {
        let dir = test_dir();
        let path = dir.join("state.json");
        atomic_write(&path, br#"{"version":1}"#).unwrap();
        atomic_write(&path, br#"{"version":2}"#).unwrap();

        remove_document(&path).unwrap();
        assert!(!path.exists());
        assert!(!backup_path(&path).exists());
        remove_document(&path).unwrap();
    }

    #[test]
    fn quarantine_moves_invalid_content_without_overwriting_it() {
        let dir = test_dir();
        let path = dir.join("state.json");
        fs::create_dir_all(&*dir).unwrap();
        fs::write(&path, b"invalid").unwrap();

        let quarantined = quarantine_file(&path).unwrap().unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(quarantined).unwrap(), b"invalid");
    }

    /// CORR-47: a second quarantine of the same document (as after a
    /// restart, when the process-local counter starts over) must not replace
    /// the earlier quarantined file.
    #[test]
    fn quarantine_names_never_collide_across_runs() {
        let dir = test_dir();
        let path = dir.join("state.json");
        fs::create_dir_all(&*dir).unwrap();

        // Occupy the names the next quarantine would pick (this second and
        // the next, in case the clock ticks over) as if an earlier run left
        // them behind with the same pid and sequence.
        let pid = std::process::id();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let sequence = TEMP_SEQUENCE.load(Ordering::Relaxed);
        let occupied: Vec<PathBuf> = [now, now + 1]
            .iter()
            .map(|ts| dir.join(format!(".state.json.invalid-{pid}-{ts}-{sequence}")))
            .collect();
        for existing in &occupied {
            fs::write(existing, b"earlier run").unwrap();
        }

        fs::write(&path, b"current").unwrap();
        let quarantined = quarantine_file(&path).unwrap().unwrap();

        assert!(!occupied.contains(&quarantined));
        assert_eq!(fs::read(&quarantined).unwrap(), b"current");
        for existing in &occupied {
            assert_eq!(fs::read(existing).unwrap(), b"earlier run");
        }
    }
}
