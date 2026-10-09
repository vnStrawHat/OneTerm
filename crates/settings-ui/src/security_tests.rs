use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// A vault header at the floor parameters (`crates/vault`'s fixture).
const V1: &str = include_str!("../../vault/tests/fixtures/persistence/vault-v1.json");

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Removes the directory on drop, including when an assertion panics
/// (`BUG-0083`).
struct TemporaryDirectory(PathBuf);

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

fn dir_with(vault_json: Option<&str>) -> TemporaryDirectory {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "oneterm-security-page-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&path).unwrap();
    if let Some(raw) = vault_json {
        std::fs::write(path.join("vault.json"), raw).unwrap();
    }
    TemporaryDirectory(path)
}

fn rows_for(vault_json: Option<&str>, read_only: bool) -> Rows {
    let dir = dir_with(vault_json);
    rows(&VaultState::load(dir.to_path_buf(), read_only))
}

#[test]
fn without_a_vault_the_page_offers_set_up_only() {
    let rows = rows_for(None, false);
    assert_eq!(rows.status, "Not set up.");
    assert_eq!(rows.primary, Some(Primary::SetUp));
    assert!(!rows.change && !rows.lock && !rows.reset);
}

#[test]
fn a_locked_vault_offers_unlock_change_lock_and_reset() {
    let rows = rows_for(Some(V1), false);
    assert!(rows.status.starts_with("Set, locked."));
    assert_eq!(rows.primary, Some(Primary::Unlock));
    assert!(rows.change && rows.reset);
    // `US-0159`: Lock Now covers the screen, which does not need the key.
    assert!(rows.lock);
}

#[test]
fn a_damaged_vault_offers_reset_only() {
    let rows = rows_for(Some("not json"), false);
    assert!(rows.status.contains("damaged"));
    assert_eq!(rows.primary, None);
    assert!(rows.reset);
    assert!(!rows.change && !rows.lock);
}

/// `DEC-0019` M4 and intake Q5: the elevated window says who manages the
/// password, never offers a write, and still unlocks.
#[test]
fn the_elevated_page_is_read_only_but_unlocks() {
    let rows = rows_for(None, true);
    assert!(rows.status.ends_with(ELEVATED_NOTE));
    assert_eq!(rows.primary, None);
    assert!(!rows.change && !rows.reset);

    let rows = rows_for(Some(V1), true);
    assert!(rows.status.ends_with(ELEVATED_NOTE));
    assert_eq!(rows.primary, Some(Primary::Unlock));
    assert!(!rows.change && !rows.reset);
    assert!(rows.lock, "the elevated window locks too (intake Q5)");

    let rows = rows_for(Some("not json"), true);
    assert!(!rows.reset);
}
