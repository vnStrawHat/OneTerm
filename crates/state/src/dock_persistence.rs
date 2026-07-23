//! Typed persistence model for `docks.json`.
//!
//! The shell owns the serialized dock layout while feature crates own their
//! optional extension state. This module provides the single read/update API so
//! callers do not patch unrelated JSON fields independently.

use std::collections::BTreeMap;
use std::io;

use oneterm_core::{SftpTableState, migrate_json_value, set_schema_version, update_json_file};
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
    /// Whether the active tab panel shows its expand/collapse affordance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_button_visible: Option<bool>,
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
            toggle_button_visible: None,
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

/// Atomically update the complete dock document under the shared file lock.
pub fn update_dock_document(
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<()> {
    update_dock_document_at(&state_file(), update)
}

/// Atomically update a dock document at an explicit path.
pub fn update_dock_document_at(
    path: &std::path::Path,
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<()> {
    update_json_file(path, |value| {
        let mut document = if value.as_object().is_some_and(|object| object.is_empty()) {
            DockDocument::default()
        } else {
            parse_document(value.clone())?
        };
        update(&mut document)?;
        let mut serialized = serde_json::to_value(document).map_err(io::Error::other)?;
        set_schema_version(&mut serialized, CURRENT_SCHEMA_VERSION)?;
        *value = serialized;
        Ok(())
    })
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
        document.toggle_button_visible = Some(false);
        document.sftp_table_state = Some(SftpTableState {
            column_widths: HashMap::from([("name".into(), 240.0)]),
            column_visibility: HashMap::from([("owner".into(), false)]),
        });

        let json = serde_json::to_string(&document).unwrap();
        let restored: DockDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.dock_state::<TestDockState>().unwrap(), state);
        assert_eq!(restored.zoomed_panel.as_deref(), Some("session"));
        assert_eq!(restored.toggle_button_visible, Some(false));
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
}
