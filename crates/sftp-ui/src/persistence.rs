//! Persistence for the SFTP table state — save/read column state (width, visibility)
//! to `docks.json` (the `sftp_table_state` field).
//!
//! Same pattern as `workspace/persistence.rs`: inject the field's JSON value into
//! `docks.json` without modifying `DockAreaState`. The workspace `save_state` is
//! patched to preserve this field when rewriting.

use anyhow::{Context as _, Result};

use oneterm_state::paths::{SFTP_TABLE_STATE_FIELD, state_file};

use super::types::SftpTableStateJson;

/// Read `sftp_table_state` from `docks.json`. `None` if the file/field does not exist
/// or fails to parse.
pub(crate) fn read_sftp_table_state() -> Option<SftpTableStateJson> {
    let raw = std::fs::read_to_string(state_file()).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get(SFTP_TABLE_STATE_FIELD)
        .and_then(|v| serde_json::from_value::<SftpTableStateJson>(v.clone()).ok())
}

/// Write `sftp_table_state` to `docks.json` — read the existing file, inject the field,
/// rewrite. Does not touch other fields (dock state, zoom, toggle, ...).
pub(crate) fn write_sftp_table_state(state: &SftpTableStateJson) -> Result<()> {
    let raw = std::fs::read_to_string(state_file()).unwrap_or_else(|_| "{}".to_string());
    let mut val: serde_json::Value = serde_json::from_str(&raw).context("parse docks.json")?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert(SFTP_TABLE_STATE_FIELD.into(), serde_json::to_value(state)?);
    }
    let json = serde_json::to_string_pretty(&val)?;
    std::fs::write(state_file(), json)?;
    Ok(())
}
