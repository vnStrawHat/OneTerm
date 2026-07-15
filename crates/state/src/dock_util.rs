//! Shared DockArea traversal helpers.
//!
//! These walk the gpui-component `DockArea` tree and are used by both the shell
//! (zoom persistence, Find) and feature crates (status-bar metrics). They live in
//! this low crate so neither the shell nor the features need to depend on each
//! other to share them.

use gpui::{App, Entity, Window};
use gpui_component::dock::{DockArea, DockItem, TabPanel};

/// Set the Right Dock open/closed (no-op if there is no right dock).
///
/// Used by the "Auto-hide Right Dock on Local Shell" feature: the shell's action
/// handler and a terminal panel's `set_active` hook both call this. Generic over
/// [`gpui::AppContext`] so it works from a `Context<T>` or an `App`.
pub fn set_right_dock_open<C: gpui::AppContext>(
    dock_area: &Entity<DockArea>,
    open: bool,
    window: &mut Window,
    cx: &mut C,
) {
    let right = cx.read_entity(dock_area, |da, _| da.right_dock().cloned());
    if let Some(right) = right {
        right.update(cx, |dock, cx| {
            if dock.is_open() != open {
                dock.set_open(open, window, cx);
            }
        });
    }
}

/// Walk the entire Dock tree (center + 3 docks) → collect every `Entity<TabPanel>`.
pub fn collect_tab_panels(dock_area: &DockArea, cx: &App) -> Vec<Entity<TabPanel>> {
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
pub fn find_tab_by_panel_name(
    dock_area: &DockArea,
    name: &str,
    cx: &App,
) -> Option<Entity<TabPanel>> {
    collect_tab_panels(dock_area, cx)
        .into_iter()
        .find(|tp| tp.read(cx).active_panel(cx).map(|p| p.panel_name(cx)) == Some(name))
}
