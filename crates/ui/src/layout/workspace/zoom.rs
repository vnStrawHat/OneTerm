//! Persist trạng thái Zoom (fullscreen) của các panel trong Dock.
//!
//! `gpui-component` không serialize `TabPanel.zoomed` (private field) vào
//! `DockAreaState`, nên zoom bị mất khi restart. Module này bù bằng cách:
//!
//! 1. Subscribe `PanelEvent::ZoomIn`/`ZoomOut` trên mỗi `TabPanel` để theo dõi
//!    panel nào đang zoom (mirror state — vì `zoomed` không đọc được từ ngoài).
//! 2. Khi save (`save_layout` / `on_app_quit`), ghi `panel_name` của panel đang
//!    zoom vào `docks.json` (field `zoomed_panel`, inject vào JSON value — không
//!    động tới struct `DockAreaState`).
//! 3. Khi load, đọc `zoomed_panel` → tìm TabPanel có active panel trùng tên →
//!    focus + dispatch `ToggleZoom` để zoom lại (qua đúng code path, toolbar state
//!    nhất quán).

use gpui::{App, Entity};
use gpui_component::dock::{DockArea, DockItem, TabPanel};

/// Tên field JSON lưu panel đang zoom trong `docks.json`.
pub(crate) const ZOOM_FIELD: &str = "zoomed_panel";

/// Duyệt toàn bộ cây Dock (center + 3 docks) → collect mọi `Entity<TabPanel>`.
pub(crate) fn collect_tab_panels(
    dock_area: &DockArea,
    cx: &App,
) -> Vec<Entity<TabPanel>> {
    let mut out = Vec::new();
    visit_item(dock_area.center(), &mut out);
    for dock in [dock_area.left_dock(), dock_area.right_dock(), dock_area.bottom_dock()]
        .into_iter()
        .flatten()
    {
        visit_item(dock.read(cx).panel(), &mut out);
    }
    out
}

fn visit_item(item: &DockItem, out: &mut Vec<Entity<TabPanel>>) {
    match item {
        DockItem::Tabs { view, .. } => out.push(view.clone()),
        DockItem::Split { items, .. } => {
            for it in items {
                visit_item(it, out);
            }
        }
        _ => {}
    }
}

/// Tìm TabPanel đầu tiên có active panel trùng `name`.
pub(crate) fn find_tab_by_panel_name(
    dock_area: &DockArea,
    name: &str,
    cx: &App,
) -> Option<Entity<TabPanel>> {
    collect_tab_panels(dock_area, cx)
        .into_iter()
        .find(|tp| tp.read(cx).active_panel(cx).map(|p| p.panel_name(cx)) == Some(name))
}
