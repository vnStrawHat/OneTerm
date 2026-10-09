//! Shared test fixtures: a self-removing temporary directory and cheap KDF
//! parameters.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::KdfParams;

/// Below the floor on purpose: unit tests derive many keys, and the floor
/// only guards headers read from disk.
pub(crate) const FAST_PARAMS: KdfParams = KdfParams {
    memory_kib: 64,
    iterations: 1,
    parallelism: 1,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Removes the directory on drop, including when an assertion panics
/// (`BUG-0083`: a trailing `remove_dir_all` never runs after a failed
/// assertion).
pub(crate) struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        // Best effort: nothing in a destructor can report a failed cleanup.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TemporaryDirectory {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

/// A fresh directory, distinct per call, process and run; created empty.
pub(crate) fn test_dir() -> TemporaryDirectory {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "oneterm-vault-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    TemporaryDirectory(path)
}
