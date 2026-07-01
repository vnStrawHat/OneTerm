//! Persist the Zoom (fullscreen) state of panels in the Dock.
//!
//! `gpui-component` does not serialize `TabPanel.zoomed` (a private field) into
//! `DockAreaState`, so the zoom is lost on restart. This module compensates by:
//!
//! 1. Subscribing to `PanelEvent::ZoomIn`/`ZoomOut` on each `TabPanel` to track
//!    which panel is zoomed (a mirror state — since `zoomed` is not readable from outside).
//! 2. On save (`save_layout` / `on_app_quit`), writing the `panel_name` of the zoomed
//!    panel into `docks.json` (field `zoomed_panel`, injected into the JSON value — without
//!    touching the `DockAreaState` struct).
//! 3. On load, reading `zoomed_panel` → finding the TabPanel whose active panel matches the
//!    name → focus + dispatch `ToggleZoom` to zoom it again (via the proper code path, with
//!    consistent toolbar state).

use gpui::{App, Entity};
use gpui_component::dock::{DockArea, DockItem, TabPanel};

/// Name of the JSON field storing the zoomed panel in `docks.json`.
pub(crate) const ZOOM_FIELD: &str = "zoomed_panel";

/// Walk the entire Dock tree (center + 3 docks) → collect every `Entity<TabPanel>`.
pub(crate) fn collect_tab_panels(dock_area: &DockArea, cx: &App) -> Vec<Entity<TabPanel>> {
    let mut out = Vec::new();
    visit_item(dock_area.center(), &mut out);
    for dock in [
        dock_area.left_dock(),
        dock_area.right_dock(),
        dock_area.bottom_dock(),
    ]
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

/// Find the first TabPanel whose active panel matches `name`.
pub(crate) fn find_tab_by_panel_name(
    dock_area: &DockArea,
    name: &str,
    cx: &App,
) -> Option<Entity<TabPanel>> {
    collect_tab_panels(dock_area, cx)
        .into_iter()
        .find(|tp| tp.read(cx).active_panel(cx).map(|p| p.panel_name(cx)) == Some(name))
}
