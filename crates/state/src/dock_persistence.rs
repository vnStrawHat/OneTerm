//! Typed persistence model for `docks.json`.
//!
//! The shell owns the serialized dock layout while feature crates own their
//! optional extension state. This module provides the single read/update API so
//! callers do not patch unrelated JSON fields independently.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;

use oneterm_core::{
    SftpTableState, migrate_json_value, quarantine_file, set_schema_version, update_json_file,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::paths::state_file;

const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Complete OneTerm-owned `docks.json` document.
#[derive(Debug, Serialize, Deserialize)]
pub struct DockDocument {
    /// Version of the complete dock document schema.
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    /// Fields owned by gpui-component's dock layout model.
    #[serde(flatten)]
    dock_fields: BTreeMap<String, Value>,
    /// Name of the currently zoomed panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed_panel: Option<String>,
    /// Persisted SFTP table column widths and visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sftp_table_state: Option<SftpTableState>,
}

fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

impl Default for DockDocument {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            dock_fields: BTreeMap::new(),
            zoomed_panel: None,
            sftp_table_state: None,
        }
    }
}

fn parse_document(value: Value) -> io::Result<DockDocument> {
    let value = migrate_json_value(value, CURRENT_SCHEMA_VERSION, "docks.json", |_, value| {
        if !value.is_object() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "docks.json schema must be an object",
            ));
        }
        Ok(value)
    })?;
    serde_json::from_value(value).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
impl DockDocument {
    /// Create a document from a serializable dock layout.
    pub fn from_dock_state<T: Serialize>(state: &T) -> serde_json::Result<Self> {
        let Value::Object(fields) = serde_json::to_value(state)? else {
            return Err(serde_json::Error::io(io::Error::new(
                io::ErrorKind::InvalidData,
                "dock state must serialize as an object",
            )));
        };
        Ok(Self {
            dock_fields: fields.into_iter().collect(),
            ..Self::default()
        })
    }

    /// Deserialize the gpui-component dock layout portion of this document.
    pub fn dock_state<T: DeserializeOwned>(&self) -> serde_json::Result<T> {
        let fields = self.dock_fields.clone().into_iter().collect();
        serde_json::from_value(Value::Object(fields))
    }
}

/// Read and parse the complete dock document.
pub fn read_dock_document() -> io::Result<DockDocument> {
    read_dock_document_from(&state_file())
}

/// Read and parse a dock document from an explicit path.
///
/// The path-taking variant keeps persistence tests independent from the process
/// configuration directory while the production wrapper retains the normal path.
pub fn read_dock_document_from(path: &std::path::Path) -> io::Result<DockDocument> {
    let raw = std::fs::read_to_string(path)?;
    parse_document(
        serde_json::from_str(&raw)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
    )
}

/// How an update transaction reached the persisted document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockUpdateOutcome {
    /// The existing document (or an absent file) was updated in place.
    Updated,
    /// The existing file was not a valid dock document. It was moved aside
    /// (`quarantined` is the sibling it was renamed to, `None` if it vanished in
    /// between) and the update was applied to `DockDocument::default()`.
    ///
    /// Callers should log this at `warn` level so the reset is diagnosable; this
    /// crate has no logging facility of its own.
    RecoveredFromInvalidData { quarantined: Option<PathBuf> },
}

/// Atomically update the complete dock document under the shared file lock.
pub fn update_dock_document(
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<DockUpdateOutcome> {
    update_dock_document_at(&state_file(), update)
}

/// Atomically update a dock document at an explicit path.
///
/// An unreadable document (`InvalidData`, e.g. truncated by a crash) does not
/// disable saving: it is quarantined next to the original path and the update
/// is applied to a default document, reported as
/// [`DockUpdateOutcome::RecoveredFromInvalidData`].
pub fn update_dock_document_at(
    path: &std::path::Path,
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<DockUpdateOutcome> {
    // The closure below only consumes `update` once the existing document has
    // parsed, so a parse failure leaves it available for the recovery attempt.
    let mut update = Some(update);
    let invalid_data = match update_json_file(path, |value| {
        let mut document = if value.as_object().is_some_and(|object| object.is_empty()) {
            DockDocument::default()
        } else {
            parse_document(value.clone())?
        };
        let update = update.take().ok_or_else(|| {
            io::Error::other("dock document update callback was already consumed")
        })?;
        apply_update(value, &mut document, update)
    }) {
        Ok(()) => return Ok(DockUpdateOutcome::Updated),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => error,
        Err(error) => return Err(error),
    };

    // `update` was consumed only when the document parsed, so an `InvalidData`
    // raised by the caller's own update is not a corrupt file: pass it through
    // untouched instead of quarantining a healthy document.
    let Some(update) = update.take() else {
        return Err(invalid_data);
    };
    // The transaction lock is released here; `quarantine_file` takes its own.
    let quarantined = quarantine_file(path)?;
    update_json_file(path, |value| {
        let mut document = DockDocument::default();
        apply_update(value, &mut document, update)
    })?;
    Ok(DockUpdateOutcome::RecoveredFromInvalidData { quarantined })
}

/// Run `update` on `document` and serialize the result into `value`.
fn apply_update(
    value: &mut Value,
    document: &mut DockDocument,
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<()> {
    update(document)?;
    let mut serialized = serde_json::to_value(&*document).map_err(io::Error::other)?;
    set_schema_version(&mut serialized, CURRENT_SCHEMA_VERSION)?;
    *value = serialized;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestDockState {
        version: u32,
        right_dock_open: bool,
    }

    #[test]
    fn typed_document_roundtrips_layout_and_feature_state() {
        let state = TestDockState {
            version: 7,
            right_dock_open: true,
        };
        let mut document = DockDocument::from_dock_state(&state).unwrap();
        document.zoomed_panel = Some("session".into());
        document.sftp_table_state = Some(SftpTableState {
            column_widths: HashMap::from([("name".into(), 240.0)]),
            column_visibility: HashMap::from([("owner".into(), false)]),
        });

        let json = serde_json::to_string(&document).unwrap();
        let restored: DockDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.dock_state::<TestDockState>().unwrap(), state);
        assert_eq!(restored.zoomed_panel.as_deref(), Some("session"));
        assert_eq!(
            restored.sftp_table_state.unwrap().column_widths.get("name"),
            Some(&240.0)
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    #[test]
    fn explicit_path_updates_are_isolated_and_atomic() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-dock-document-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let path = directory.join("docks.json");
        update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("terminal".into());
            Ok(())
        })
        .unwrap();
        let restored = read_dock_document_from(&path).unwrap();
        assert_eq!(restored.zoomed_panel.as_deref(), Some("terminal"));
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        std::fs::write(&path, b"not-json").unwrap();
        assert_eq!(
            read_dock_document_from(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn invalid_document_is_quarantined_and_updates_keep_working() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-dock-recovery-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("docks.json");
        std::fs::write(&path, b"{ not json").unwrap();

        let outcome = update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("terminal".into());
            Ok(())
        })
        .unwrap();

        let DockUpdateOutcome::RecoveredFromInvalidData { quarantined } = outcome else {
            panic!("expected recovery, got {outcome:?}");
        };
        let quarantined = quarantined.expect("the corrupt file must be moved aside");
        assert_eq!(quarantined.parent(), Some(directory.as_path()));
        assert_eq!(std::fs::read(&quarantined).unwrap(), b"{ not json");
        let restored = read_dock_document_from(&path).unwrap();
        assert_eq!(restored.zoomed_panel.as_deref(), Some("terminal"));
        assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);

        // A well-formed document that is not a dock document is recovered too.
        std::fs::write(&path, b"[1, 2, 3]").unwrap();
        let outcome = update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("sftp".into());
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            outcome,
            DockUpdateOutcome::RecoveredFromInvalidData {
                quarantined: Some(_)
            }
        ));
        let restored = read_dock_document_from(&path).unwrap();
        assert_eq!(restored.zoomed_panel.as_deref(), Some("sftp"));
        assert!(restored.sftp_table_state.is_none());

        // Subsequent updates on the healthy document are plain updates.
        let outcome = update_dock_document_at(&path, |_| Ok(())).unwrap();
        assert_eq!(outcome, DockUpdateOutcome::Updated);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn legacy_fixture_migrates_during_shared_update() {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-dock-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("docks.json");
        std::fs::write(
            &path,
            include_str!("../tests/fixtures/persistence/docks-v0.json"),
        )
        .unwrap();
        let legacy = read_dock_document_from(&path).unwrap();
        assert_eq!(legacy.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(legacy.zoomed_panel.as_deref(), Some("terminal"));
        update_dock_document_at(&path, |_| Ok(())).unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], CURRENT_SCHEMA_VERSION);
        assert_eq!(
            read_dock_document_from(&path).unwrap().schema_version,
            CURRENT_SCHEMA_VERSION
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    /// A fresh per-test directory; tests never touch the real configuration directory.
    fn temp_directory(label: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "oneterm-dock-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn concurrent_updates_from_many_threads_are_all_applied() {
        let directory = temp_directory("concurrent");
        let path = directory.join("docks.json");
        let threads: Vec<_> = (0..8u32)
            .map(|worker| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for round in 0..5u32 {
                        let outcome = update_dock_document_at(&path, |document| {
                            // Every writer touches a distinct dock field, so a
                            // lost update would leave a hole in the final document.
                            document
                                .dock_fields
                                .insert(format!("worker-{worker}-{round}"), Value::Bool(true));
                            document.zoomed_panel = Some(format!("worker-{worker}"));
                            Ok(())
                        })
                        .unwrap();
                        assert_eq!(outcome, DockUpdateOutcome::Updated);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let restored = read_dock_document_from(&path).unwrap();
        assert_eq!(restored.dock_fields.len(), 8 * 5);
        assert!(restored.zoomed_panel.is_some());
        assert_eq!(restored.schema_version, CURRENT_SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn update_preserves_the_previous_document_as_backup() {
        let directory = temp_directory("backup");
        let path = directory.join("docks.json");
        update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("first".into());
            Ok(())
        })
        .unwrap();
        let first = std::fs::read_to_string(&path).unwrap();

        update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("second".into());
            Ok(())
        })
        .unwrap();

        let backup = read_dock_document_from(&directory.join("docks.bak")).unwrap();
        assert_eq!(backup.zoomed_panel.as_deref(), Some("first"));
        assert_eq!(
            std::fs::read_to_string(directory.join("docks.bak")).unwrap(),
            first
        );
        assert_eq!(
            read_dock_document_from(&path)
                .unwrap()
                .zoomed_panel
                .as_deref(),
            Some("second")
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn quarantine_keeps_the_invalid_bytes_and_the_last_good_backup() {
        let directory = temp_directory("quarantine-backup");
        let path = directory.join("docks.json");
        update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("good".into());
            Ok(())
        })
        .unwrap();
        // A second write makes the healthy document the `.bak` of the file we corrupt.
        update_dock_document_at(&path, |_| Ok(())).unwrap();
        std::fs::write(&path, b"{ truncated").unwrap();

        let outcome = update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("recovered".into());
            Ok(())
        })
        .unwrap();

        let DockUpdateOutcome::RecoveredFromInvalidData { quarantined } = outcome else {
            panic!("expected recovery, got {outcome:?}");
        };
        let quarantined = quarantined.expect("invalid file is moved aside");
        assert_eq!(std::fs::read(&quarantined).unwrap(), b"{ truncated");
        assert_eq!(
            read_dock_document_from(&directory.join("docks.bak"))
                .unwrap()
                .zoomed_panel
                .as_deref(),
            Some("good"),
            "the last good backup survives quarantine"
        );
        assert_eq!(
            read_dock_document_from(&path)
                .unwrap()
                .zoomed_panel
                .as_deref(),
            Some("recovered")
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn current_schema_document_loads_idempotently() {
        let directory = temp_directory("idempotent");
        let path = directory.join("docks.json");
        update_dock_document_at(&path, |document| {
            document.zoomed_panel = Some("terminal".into());
            Ok(())
        })
        .unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        update_dock_document_at(&path, |_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
        let _ = std::fs::remove_dir_all(directory);
    }
}
