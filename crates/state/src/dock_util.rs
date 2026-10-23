//! Shared DockArea traversal helpers.
//!
//! These walk the gpui-component `DockArea` tree and are used by both the shell
//! (the right dock) and feature crates (status-bar metrics). They live in this low
//! crate so neither the shell nor the features need to depend on each other to
//! share them.

use std::sync::Arc;

use gpui::{App, Entity, Window};
use gpui_component::dock::{BasePanelView, DockArea, DockPlacement, PaneNode, PaneRef};

/// Set the Right Dock open/closed (no-op if there is no right dock).
///
/// Used by action handlers that reveal the right dock (Add Session, Add SFTP
/// Browser, the right dock following the tab kind). Generic over [`gpui::AppContext`] so it works from a
/// `Context<T>` or an `App`.
pub fn set_right_dock_open<C: gpui::AppContext>(
    dock_area: &Entity<DockArea>,
    open: bool,
    window: &mut Window,
    cx: &mut C,
) {
    dock_area.update(cx, |dock_area, cx| {
        if dock_area.has_dock(DockPlacement::Right)
            && dock_area.is_dock_open(DockPlacement::Right) != open
        {
            dock_area.toggle_dock(DockPlacement::Right, window, cx);
        }
    });
}

/// Return the active panel of every tab group in dock order.
pub fn active_tab_panels(dock_area: &DockArea, cx: &App) -> Vec<Arc<dyn BasePanelView>> {
    let mut panels = Vec::new();
    for placement in [
        DockPlacement::Center,
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ] {
        if let Some(tree) = dock_area.layout(placement) {
            visit_active_tabs(tree.root(), dock_area, cx, &mut panels);
        }
    }
    panels
}

fn visit_active_tabs(
    node: &PaneNode,
    dock_area: &DockArea,
    cx: &App,
    panels: &mut Vec<Arc<dyn BasePanelView>>,
) {
    match node.kind() {
        PaneRef::Tabs {
            panels: ids,
            active_ix,
        } => {
            if let Some(panel) = ids.get(active_ix).and_then(|id| dock_area.panel(*id))
                && panel.visible(cx)
            {
                panels.push(panel.clone());
            }
        }
        PaneRef::Split { children, .. } => {
            for child in children {
                visit_active_tabs(child, dock_area, cx, panels);
            }
        }
    }
}
