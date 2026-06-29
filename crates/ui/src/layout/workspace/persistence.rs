//! Layout persistence — load / save dock state.

use anyhow::{Context as _, Result};
use gpui::{Context, Edges, Entity, PromptLevel, Window};
use gpui_component::dock::{DockArea, DockAreaState};

use super::{MAIN_DOCK_VERSION, SFTP_TABLE_STATE_FIELD, STATE_FILE};

impl super::MyTermWorkspace {
    /// Load layout từ file — dùng để giữ right dock + settings.
    pub(crate) fn load_layout(
        dock_area: Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let json = std::fs::read_to_string(STATE_FILE)?;
        let state = serde_json::from_str::<DockAreaState>(&json)?;

        if state.version != Some(MAIN_DOCK_VERSION) {
            let answer = window.prompt(
                PromptLevel::Info,
                "The default main layout has been updated.\n\
                Do you want to reset the layout to default?",
                None,
                &["Yes", "No"],
                cx,
            );

            let weak_dock_area = dock_area.downgrade();
            cx.spawn_in(window, async move |this, window| {
                if answer.await == Ok(0) {
                    _ = this.update_in(window, |_, window, cx| {
                        super::layout::reset_default_layout(weak_dock_area, window, cx);
                    });
                }
            })
            .detach();
        }

        dock_area.update(cx, |dock_area, cx| {
            dock_area.load(state, window, cx).context("load layout")?;
            dock_area.set_dock_collapsible(
                Edges {
                    right: true,
                    ..Default::default()
                },
                window,
                cx,
            );
            Ok::<(), anyhow::Error>(())
        })
    }
}

/// Save dock state ra file.
///
/// `zoomed_panel`: tên panel đang zoom (fullscreen) — inject vào JSON value
/// của `docks.json` (field `zoomed_panel`). `None` → xoá field (panel không zoom).
/// `toggle_button_visible`: hiện/ẩn nút expand/collapse trên TabPanel — inject
/// vào JSON (field `toggle_button_visible`).
/// Không sửa struct `DockAreaState`.
/// `trigger`: chuỗi mô tả nguồn kích hoạt ghi (vd "debounce", "on_app_quit",
/// "zoom_in", "zoom_out", "reset_center_only", "reset_default_layout").
pub(crate) fn save_state(
    state: &DockAreaState,
    zoomed_panel: Option<&str>,
    toggle_button_visible: bool,
    trigger: &str,
) -> Result<()> {
    let mut val = serde_json::to_value(state)?;

    // Preserve `sftp_table_state` từ file hiện có trước khi overwrite —
    // `views/sftp/persistence.rs` quản lý field này độc lập với DockAreaState.
    let mut raw_sftp_state: Option<serde_json::Value> = std::fs::read_to_string(STATE_FILE)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get(SFTP_TABLE_STATE_FIELD).cloned());
    let right_dock_open = val
        .get("right_dock")
        .and_then(|d| d.get("open"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    tracing::info!(
        "Save layout [trigger={trigger}] → zoomed_panel={zoomed_panel:?}, toggle_button_visible={toggle_button_visible}, right_dock_open={right_dock_open}",
    );
    eprintln!(
        "[dock-save] trigger={trigger} | zoomed={zoomed_panel:?} | toggle_btn={toggle_button_visible} | right_open={right_dock_open}",
    );
    if let Some(obj) = val.as_object_mut() {
        match zoomed_panel {
            Some(name) => {
                obj.insert(
                    super::zoom::ZOOM_FIELD.into(),
                    serde_json::Value::String(name.into()),
                );
            }
            None => {
                obj.remove(super::zoom::ZOOM_FIELD);
            }
        }
        obj.insert(
            super::TOGGLE_BUTTON_VISIBLE_FIELD.into(),
            serde_json::Value::Bool(toggle_button_visible),
        );

        // Preserve `sftp_table_state` (column widths + visibility) nếu có sẵn
        // trong file — tránh bị overwrite khi workspace save lại layout.
        // (Ghi bởi `views/sftp/persistence.rs`.)
        let preserved = raw_sftp_state.take();
        if let Some(v) = preserved {
            obj.insert(SFTP_TABLE_STATE_FIELD.into(), v);
        }
    }
    let json = serde_json::to_string_pretty(&val)?;
    std::fs::write(STATE_FILE, json)?;
    Ok(())
}

/// Đọc tên panel đang zoom (fullscreen) từ `docks.json` trước khi layout bị
/// reset (center luôn reset về 1 tab mới). Trả về `None` nếu file không tồn
/// tại hoặc chưa có panel nào zoom.
pub(crate) fn read_zoomed_panel() -> Option<String> {
    let raw = std::fs::read_to_string(STATE_FILE).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get(super::zoom::ZOOM_FIELD)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Đọc `toggle_button_visible` từ `docks.json`. Trả về `None` nếu file không
/// tồn tại hoặc chưa có field.
pub(crate) fn read_toggle_button_visible() -> Option<bool> {
    let raw = std::fs::read_to_string(STATE_FILE).ok()?;
    let val: serde_json::Value = serde_json::from_str(&raw).ok()?;
    val.get(super::TOGGLE_BUTTON_VISIBLE_FIELD)
        .and_then(|v| v.as_bool())
}

#[cfg(test)]
mod tests {
    use gpui_component::dock::DockAreaState;

    #[test]
    fn zoomed_panel_field_roundtrips_and_keeps_state_deserializable() {
        let state = DockAreaState::default();
        let mut val = serde_json::to_value(&state).unwrap();
        val.as_object_mut().unwrap().insert(
            super::super::zoom::ZOOM_FIELD.into(),
            serde_json::Value::String("session".into()),
        );
        let json = serde_json::to_string_pretty(&val).unwrap();

        // Extra field must NOT break DockAreaState deserialization.
        let parsed: DockAreaState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);

        // Field readable back.
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            val[super::super::zoom::ZOOM_FIELD].as_str(),
            Some("session")
        );
    }

    #[test]
    fn absent_zoomed_panel_is_none() {
        let json = serde_json::to_string_pretty(&DockAreaState::default()).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(val.get(super::super::zoom::ZOOM_FIELD).is_none());
    }

    #[test]
    fn toggle_button_visible_field_roundtrips() {
        let state = DockAreaState::default();
        let mut val = serde_json::to_value(&state).unwrap();
        val.as_object_mut().unwrap().insert(
            super::super::TOGGLE_BUTTON_VISIBLE_FIELD.into(),
            serde_json::Value::Bool(false),
        );
        let json = serde_json::to_string_pretty(&val).unwrap();

        // Extra field must NOT break DockAreaState deserialization.
        let parsed: DockAreaState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);

        // Field readable back.
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            val[super::super::TOGGLE_BUTTON_VISIBLE_FIELD].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn sftp_table_state_field_coexists_with_dock_state() {
        // `sftp_table_state` là field JSON inject bởi `views/sftp/persistence.rs`.
        // Phải không break deserialize `DockAreaState` và phải đọc lại được.
        let state = DockAreaState::default();
        let mut val = serde_json::to_value(&state).unwrap();
        val.as_object_mut().unwrap().insert(
            super::super::SFTP_TABLE_STATE_FIELD.into(),
            serde_json::json!({
                "column_widths": { "name": 320.0, "size": 80.0 },
                "column_visibility": { "name": true, "permissions": false }
            }),
        );
        let json = serde_json::to_string_pretty(&val).unwrap();

        let parsed: DockAreaState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, state);

        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sftp = &val[super::super::SFTP_TABLE_STATE_FIELD];
        assert_eq!(sftp["column_visibility"]["permissions"].as_bool(), Some(false));
        assert_eq!(sftp["column_widths"]["name"].as_f64(), Some(320.0));
    }
}
