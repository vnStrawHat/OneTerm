//! Default workspace layout construction.

use std::sync::Arc;

use gpui::{App, Window, px};
use gpui_component::dock::{DockArea, DockItem};

use crate::views::{SessionPanel, SftpPanel, TerminalPanel};

use super::MAIN_DOCK_VERSION;

/// Reset chỉ center (terminal tabs) về 1 tab — giữ right dock + settings.
pub(crate) fn reset_center_only(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let weak = dock_area.clone();
    let center = DockItem::v_split(
        vec![DockItem::tabs(
            vec![Arc::new(TerminalPanel::new_entity(window, cx))],
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
        _ = super::persistence::save_state(&view.dump(cx), None);
    });
}

/// Dựng layout mặc định myTerm2: center = terminals, right_dock = Session/SFTP.
pub(crate) fn reset_default_layout(
    dock_area: gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    let weak = dock_area.clone();

    let center = DockItem::v_split(
        vec![DockItem::tabs(
            vec![Arc::new(TerminalPanel::new_entity(window, cx))],
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
            DockItem::tab(SessionPanel::new_entity(window, cx), &weak, window, cx),
            DockItem::tab(SftpPanel::new_entity(window, cx), &weak, window, cx),
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
        _ = super::persistence::save_state(&view.dump(cx), None);
    });
}
