//! Persistence cho SFTP table state — lưu/đọc trạng thái cột (width, visibility)
//! vào `docks.json` (field `sftp_table_state`).
//!
//! Cùng pattern với `workspace/persistence.rs`: inject field JSON value vào
//! `docks.json` mà không sửa `DockAreaState`. Workspace `save_state` đã được
//! patch để preserve field này khi ghi lại.

use anyhow::{Context as _, Result};

use crate::layout::workspace::{SFTP_TABLE_STATE_FIELD, STATE_FILE};

use super::types::SftpTableStateJson;

/// Đọc `sftp_table_state` từ `docks.json`. `None` nếu file/field không tồn tại
/// hoặc parse lỗi.
pub(crate) fn read_sftp_table_state() -> Option<SftpTableStateJson> {
    let raw = std::fs::read_to_string(STATE_FILE).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get(SFTP_TABLE_STATE_FIELD)
        .and_then(|v| serde_json::from_value::<SftpTableStateJson>(v.clone()).ok())
}

/// Ghi `sftp_table_state` vào `docks.json` — đọc file hiện có, inject field,
/// ghi lại. Không sửa các field khác (dock state, zoom, toggle, ...).
pub(crate) fn write_sftp_table_state(state: &SftpTableStateJson) -> Result<()> {
    let raw = std::fs::read_to_string(STATE_FILE).unwrap_or_else(|_| "{}".to_string());
    let mut val: serde_json::Value = serde_json::from_str(&raw).context("parse docks.json")?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert(
            SFTP_TABLE_STATE_FIELD.into(),
            serde_json::to_value(state)?,
        );
    }
    let json = serde_json::to_string_pretty(&val)?;
    std::fs::write(STATE_FILE, json)?;
    Ok(())
}