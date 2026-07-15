//! Default workspace layout construction.

use gpui::{App, Window, px};
use gpui_component::dock::{DockArea, DockItem};

use super::MAIN_DOCK_VERSION;

/// Reset only the center (terminal tabs) to a single tab — keep the right dock + settings.
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
    _ = dock_area.update(cx, |view, cx| {
        view.set_center(center, window, cx);
        _ = super::persistence::save_state(
            &view.dump(cx),
            None,
            toggle_button_visible,
            "reset_center_only",
        );
    });
}

/// Build the default OneTerm layout: center = terminals, right_dock = Session/SFTP.
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

    let right = DockItem::v_split(
        vec![
            DockItem::tabs(
                vec![super::build_named_panel("session", &weak, window, cx)],
                &weak,
                window,
                cx,
            ),
            DockItem::tabs(
                vec![super::build_named_panel("sftp", &weak, window, cx)],
                &weak,
                window,
                cx,
            ),
        ],
        &weak,
        window,
        cx,
    );

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
        _ = super::persistence::save_state(&view.dump(cx), None, true, "reset_default_layout");
    });
}
