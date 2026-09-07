//! Shared DockArea traversal helpers.
//!
//! These walk the gpui-component `DockArea` tree and are used by both the shell
//! (zoom persistence, Find) and feature crates (status-bar metrics). They live in
//! this low crate so neither the shell nor the features need to depend on each
//! other to share them.

use std::sync::Arc;

use gpui::{App, Entity, Window};
use gpui_component::dock::{BasePanelView, DockArea, DockPlacement, NodeId, PaneNode, PaneRef};

/// Set the Right Dock open/closed (no-op if there is no right dock).
///
/// Used by action handlers that reveal the right dock (Add Session, Add SFTP
/// Browser, mode toggle). Generic over [`gpui::AppContext`] so it works from a
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
        PaneRef::Tiles { .. } => {}
    }
}

/// Find the first tab-group node whose active panel matches `name`.
pub fn find_tab_node_by_panel_name(dock_area: &DockArea, name: &str, cx: &App) -> Option<NodeId> {
    for placement in [
        DockPlacement::Center,
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ] {
        let Some(tree) = dock_area.layout(placement) else {
            continue;
        };
        let mut found = None;
        tree.root().walk(&mut |node| {
            if found.is_some() {
                return;
            }
            let PaneRef::Tabs { panels, active_ix } = node.kind() else {
                return;
            };
            if panels
                .get(active_ix)
                .and_then(|id| dock_area.panel(*id))
                .is_some_and(|panel| panel.panel_name(cx) == name)
            {
                found = Some(node.id());
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Return the active panel name for a tab-group node.
pub fn panel_name_for_tab_node<'a>(
    dock_area: &'a DockArea,
    node_id: NodeId,
    cx: &'a App,
) -> Option<&'static str> {
    for placement in [
        DockPlacement::Center,
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ] {
        let Some(node) = dock_area
            .layout(placement)
            .and_then(|tree| tree.find_node(node_id))
        else {
            continue;
        };
        let PaneRef::Tabs { panels, active_ix } = node.kind() else {
            continue;
        };
        return panels
            .get(active_ix)
            .and_then(|id| dock_area.panel(*id))
            .map(|panel| panel.panel_name(cx));
    }
    None
}
