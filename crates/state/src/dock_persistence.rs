//! Typed persistence model for `docks.json`.
//!
//! The shell owns the serialized dock layout while feature crates own their
//! optional extension state. This module provides the single read/update API so
//! callers do not patch unrelated JSON fields independently.

use std::collections::BTreeMap;
use std::io;

use oneterm_core::{SftpTableState, update_json_file};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::paths::state_file;

/// Complete OneTerm-owned `docks.json` document.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DockDocument {
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
    let raw = std::fs::read_to_string(state_file())?;
    serde_json::from_str(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Atomically update the complete dock document under the shared file lock.
pub fn update_dock_document(
    update: impl FnOnce(&mut DockDocument) -> io::Result<()>,
) -> io::Result<()> {
    update_json_file(&state_file(), |value| {
        let mut document = if value.as_object().is_some_and(|object| object.is_empty()) {
            DockDocument::default()
        } else {
            serde_json::from_value(value.clone())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        };
        update(&mut document)?;
        *value = serde_json::to_value(document).map_err(io::Error::other)?;
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
