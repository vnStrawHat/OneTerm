//! Default workspace layout construction.

use gpui::{App, Window};
use gpui_component::dock::{DockArea, DockAreaState, DockLayout, DockPlacement};

use oneterm_state::panel_names;

use super::{DEFAULT_RIGHT_DOCK_WIDTH, MAIN_DOCK_VERSION};

/// Reset the center (terminal tabs) and re-apply the right-dock panel while
/// preserving the loaded right-dock size and open state.
///
/// `preferred_right_dock_width` is the width the saved layout asked for; it is
/// what gets written back, while the clamp decides what is applied (`US-0113`).
pub(crate) fn reset_center_only(
    dock_area: gpui::WeakEntity<DockArea>,
    preferred_right_dock_width: gpui::Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(state) = apply_center_reset(dock_area, window, cx) {
        let state = super::state_with_preferred_width(state, preferred_right_dock_width);
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
    let right = right_dock(&dock_area, window, cx)?;
    let center = DockLayout::v_split().child(DockLayout::tabs().panel_view(center_panel, cx), None);

    dock_area
        .update(cx, |view, cx| {
            let right_size = super::clamp_right_dock_width(
                view.dock_size(DockPlacement::Right)
                    .unwrap_or(DEFAULT_RIGHT_DOCK_WIDTH),
                window.viewport_size().width,
            );
            view.set_center(center, window, cx);
            match right {
                Some(right) => {
                    view.set_dock(DockPlacement::Right, right, window, cx);
                    view.set_dock_size(DockPlacement::Right, right_size, window, cx);
                    view.set_dock_collapsible(DockPlacement::Right, true, window, cx);
                }
                // Removed, not merely "not added": declining to call `set_dock`
                // leaves a dock that is already there, which is how a restored
                // `ssh_client` dock survived into an elevated window
                // (`IN-0043` MAJ-2). `remove_dock` is a no-op when there is none.
                None => view.remove_dock(DockPlacement::Right, window, cx),
            }
            view.dump(cx)
        })
        .ok()
}

/// The right dock's content, or `None` when this window has no right dock.
///
/// M1 (`DEC-0019`): an elevated window runs local shells and nothing else, so
/// the SSH Client panel is **not built** — and the caller **removes** any dock
/// that is already there rather than simply not adding one. Not adding was the
/// original gate and it was not enough: `load_layout` restores the side docks by
/// name before either builder runs, so the dock existed before anyone declined
/// to create it (`IN-0043` MAJ-2). An elevated window now also never reads
/// `docks.json` at all (`OneTermWorkspace::new`), which is the primary fix; this
/// is the second lock on the same door. `sync_right_dock_mode` and
/// `apply_right_dock_width` both early-return on `!has_dock(Right)`.
///
/// The outer `Option` is the caller's existing "the dock area is gone" signal;
/// the inner one is the elevation gate.
fn right_dock(
    dock_area: &gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Option<DockLayout>> {
    if oneterm_core::elevation::is_restricted() {
        return Some(None);
    }
    let panel = super::build_named_panel(panel_names::SSH_CLIENT, dock_area, window, cx)?;
    Some(Some(DockLayout::tabs().panel_view(panel, cx)))
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
    let Some(right) = right_dock(&dock_area, window, cx) else {
        return;
    };
    let center = DockLayout::v_split().child(DockLayout::tabs().panel_view(center_panel, cx), None);

    let default_size =
        super::clamp_right_dock_width(DEFAULT_RIGHT_DOCK_WIDTH, window.viewport_size().width);
    let saved_state = dock_area
        .update(cx, |view, cx| {
            view.set_version(Some(MAIN_DOCK_VERSION), cx);
            view.set_center(center, window, cx);
            match right {
                Some(right) => {
                    view.set_dock(DockPlacement::Right, right, window, cx);
                    view.set_dock_size(DockPlacement::Right, default_size, window, cx);
                    view.set_dock_collapsible(DockPlacement::Right, true, window, cx);
                }
                None => view.remove_dock(DockPlacement::Right, window, cx),
            }
            view.dump(cx)
        })
        .ok()
        // The default width is the preference a first launch starts from, even
        // when this window is too narrow to apply it (`US-0113`).
        .map(|state| super::state_with_preferred_width(state, DEFAULT_RIGHT_DOCK_WIDTH));
    if let Some(state) = saved_state {
        cx.background_executor()
            .spawn(async move {
                super::persistence::save_state_logged(&state, None, "reset_default_layout");
            })
            .detach();
    }
}
