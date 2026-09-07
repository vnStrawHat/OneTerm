//! Default workspace layout construction.

use gpui::{App, Window};
use gpui_component::dock::{DockArea, DockAreaState, DockLayout, DockPlacement};

use oneterm_state::panel_names;

use super::{DEFAULT_RIGHT_DOCK_WIDTH, MAIN_DOCK_VERSION};

/// Reset the center (terminal tabs) and re-apply the right-dock panel while
/// preserving the loaded right-dock size and open state.
pub(crate) fn reset_center_only(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(state) = apply_center_reset(dock_area, window, cx) {
        cx.background_executor()
            .spawn(async move {
                super::persistence::save_state_logged(&state, None, "reset_center_only");
            })
            .detach();
    }
}

/// The dock mutation behind [`reset_center_only`]; returns the resulting dock
/// state for the caller to persist (`None` when the dock area is gone).
pub(crate) fn apply_center_reset(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Option<DockAreaState> {
    let center_panel = super::build_named_panel(panel_names::TERMINAL, &dock_area, window, cx)?;
    let right_panel = super::build_named_panel(panel_names::SSH_CLIENT, &dock_area, window, cx)?;
    let center = DockLayout::v_split().child(DockLayout::tabs().panel_view(center_panel, cx), None);
    let right = DockLayout::tabs().panel_view(right_panel, cx);

    dock_area
        .update(cx, |view, cx| {
            let right_size = view
                .dock_size(DockPlacement::Right)
                .unwrap_or(DEFAULT_RIGHT_DOCK_WIDTH);
            view.set_center(center, window, cx);
            view.set_dock(DockPlacement::Right, right, window, cx);
            view.set_dock_size(DockPlacement::Right, right_size, window, cx);
            view.set_dock_collapsible(DockPlacement::Right, true, window, cx);
            view.dump(cx)
        })
        .ok()
}

/// Build the default OneTerm layout: center = terminals, right dock = SSH client.
pub(crate) fn reset_default_layout(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(center_panel) =
        super::build_named_panel(panel_names::TERMINAL, &dock_area, window, cx)
    else {
        return;
    };
    let Some(right_panel) =
        super::build_named_panel(panel_names::SSH_CLIENT, &dock_area, window, cx)
    else {
        return;
    };
    let center = DockLayout::v_split().child(DockLayout::tabs().panel_view(center_panel, cx), None);
    let right = DockLayout::tabs().panel_view(right_panel, cx);

    let saved_state = dock_area
        .update(cx, |view, cx| {
            view.set_version(Some(MAIN_DOCK_VERSION), cx);
            view.set_center(center, window, cx);
            view.set_dock(DockPlacement::Right, right, window, cx);
            view.set_dock_size(DockPlacement::Right, DEFAULT_RIGHT_DOCK_WIDTH, window, cx);
            view.set_dock_collapsible(DockPlacement::Right, true, window, cx);
            view.dump(cx)
        })
        .ok();
    if let Some(state) = saved_state {
        cx.background_executor()
            .spawn(async move {
                super::persistence::save_state_logged(&state, None, "reset_default_layout");
            })
            .detach();
    }
}
