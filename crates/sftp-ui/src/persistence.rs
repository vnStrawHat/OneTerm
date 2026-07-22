//! Persistence for the SFTP table state — save/read column state (width, visibility)
//! to `docks.json` (the `sftp_table_state` field).
//!
//! Same pattern as `workspace/persistence.rs`: inject the field's JSON value into
//! `docks.json` without modifying `DockAreaState`. The workspace `save_state` is
//! patched to preserve this field when rewriting.

use anyhow::{Context as _, Result};

use oneterm_core::{quarantine_file, update_json_file};
use oneterm_state::paths::{SFTP_TABLE_STATE_FIELD, state_file};

use super::types::SftpTableStateJson;

/// Read `sftp_table_state` from `docks.json`. `None` if the file/field does not exist
/// or fails to parse.
pub(crate) fn read_sftp_table_state() -> Option<SftpTableStateJson> {
    let raw = std::fs::read_to_string(state_file()).ok()?;
    let val: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(error) => {
            if let Err(quarantine_error) = quarantine_file(&state_file()) {
                log::warn!("failed to quarantine docks.json: {quarantine_error}");
            }
            log::warn!("docks.json parse error while reading SFTP table state: {error}");
            return None;
        }
    };
    val.get(SFTP_TABLE_STATE_FIELD)
        .and_then(|v| serde_json::from_value::<SftpTableStateJson>(v.clone()).ok())
}

/// Write `sftp_table_state` to `docks.json` — read the existing file, inject the field,
/// rewrite. Does not touch other fields (dock state, zoom, toggle, ...).
pub(crate) fn write_sftp_table_state(state: &SftpTableStateJson) -> Result<()> {
    let value = serde_json::to_value(state)?;
    update_json_file(&state_file(), |document| {
        let object = document.as_object_mut().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "docks.json root is not an object",
            )
        })?;
        object.insert(SFTP_TABLE_STATE_FIELD.into(), value);
        Ok(())
    })
    .context("write docks.json")
}
