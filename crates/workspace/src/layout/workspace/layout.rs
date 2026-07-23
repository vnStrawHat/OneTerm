//! Default workspace layout construction.

use gpui::{App, Window, px};
use gpui_component::dock::{DockArea, DockItem};

use super::MAIN_DOCK_VERSION;

/// Reset the center (terminal tabs) to a single tab AND re-apply the right
/// dock as a fresh `DockItem::Panel(SshClientPanel)`.
///
/// The right dock is rebuilt every launch because the gpui-component
/// `PanelInfo::Panel` load path round-trips back to `DockItem::tabs` (see
/// `dock::state::PanelState::to_item`), which would render the SshClientPanel
/// with an unwanted tab bar. Re-applying a fresh `DockItem::panel(...)` keeps
/// the raw chromeless rendering stable across restarts. The user's last
/// right-dock width + open/collapsed state is preserved from the just-loaded
/// dock.
pub(crate) fn reset_center_only(
    dock_area: gpui::WeakEntity<DockArea>,
    toggle_button_visible: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let weak = dock_area.clone();
    let center = DockItem::v_split(
        vec![DockItem::tabs(
            vec![super::build_named_panel("terminal", &weak, window, cx)],
            &weak,
            window,
            cx,
        )],
        &weak,
        window,
        cx,
    );
    let ssh_client_panel = super::build_named_panel("ssh_client_panel", &weak, window, cx);
    let right = DockItem::panel(ssh_client_panel);
    let mut saved_state = None;
    _ = dock_area.update(cx, |view, cx| {
        // Snapshot the loaded right dock's size + open state so the re-applied
        // `DockItem::Panel` preserves the user's last dock width + collapsed
        // state. Falls back to 480px / open when there is no prior right dock
        // (e.g. first launch, or the saved layout had no right dock).
        let (right_size, right_open) = view
            .right_dock()
            .map(|dock| {
                let d = dock.read(cx);
                (Some(d.size()), d.is_open())
            })
            .unwrap_or((Some(px(480.)), true));
        view.set_center(center, window, cx);
        view.set_right_dock(right, right_size, right_open, window, cx);
        view.set_dock_collapsible(
            gpui::Edges {
                right: true,
                ..Default::default()
            },
            window,
            cx,
        );
        saved_state = Some(view.dump(cx));
    });
    if let Some(state) = saved_state {
        cx.background_executor()
            .spawn(async move {
                super::persistence::save_state_logged(
                    &state,
                    None,
                    toggle_button_visible,
                    "reset_center_only",
                );
            })
            .detach();
    }
}

/// Build the default OneTerm layout: center = terminals, right_dock = SshClientPanel.
pub(crate) fn reset_default_layout(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let weak = dock_area.clone();

    let center = DockItem::v_split(
        vec![DockItem::tabs(
            vec![super::build_named_panel("terminal", &weak, window, cx)],
            &weak,
            window,
            cx,
        )],
        &weak,
        window,
        cx,
    );

    let ssh_client_panel = super::build_named_panel("ssh_client_panel", &weak, window, cx);
    let right = DockItem::panel(ssh_client_panel);

    let mut saved_state = None;
    _ = dock_area.update(cx, |view, cx| {
        view.set_version(MAIN_DOCK_VERSION, window, cx);
        view.set_center(center, window, cx);
        view.set_right_dock(right, Some(px(480.)), true, window, cx);
        view.set_dock_collapsible(
            gpui::Edges {
                right: true,
                ..Default::default()
            },
            window,
            cx,
        );
        saved_state = Some(view.dump(cx));
    });
    if let Some(state) = saved_state {
        cx.background_executor()
            .spawn(async move {
                super::persistence::save_state_logged(&state, None, true, "reset_default_layout");
            })
            .detach();
    }
}
