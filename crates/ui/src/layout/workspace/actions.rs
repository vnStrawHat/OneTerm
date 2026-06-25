//! Action handlers cho `MyTermWorkspace`.

use std::sync::{Arc, atomic::Ordering};

use gpui::{Context, Window};
use gpui_component::dock::{DockItem, DockPlacement};

use crate::{
    actions::{AddPanel, AddSession, AddSftpBrowser, Quit, ToggleDockToggleButton},
    views::{SessionPanel, SftpPanel, TerminalPanel},
};

impl super::MyTermWorkspace {
    /// Action handler: thêm TerminalPanel mới.
    pub(crate) fn on_action_add_panel(
        &mut self,
        action: &AddPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(TerminalPanel::new_entity(window, cx));

        // Khi tất cả tab đã đóng, center DockItem vẫn giữ entry cũ nhưng
        // TabPanel bên trong không còn panel nào → add_panel thêm vào
        // TabPanel "ma" không được render.  Phát hiện và recreate center.
        let center_empty = {
            let dock = self.dock_area.read(cx);
            Self::center_has_no_visible_panel(&dock.center(), cx)
        };

        if center_empty && matches!(action.0, DockPlacement::Center) {
            let weak = self.dock_area.downgrade();
            let center = DockItem::v_split(
                vec![DockItem::tabs(vec![panel], &weak, window, cx)],
                &weak,
                window,
                cx,
            );
            self.dock_area.update(cx, |dock_area, cx| {
                dock_area.set_center(center, window, cx);
            });
        } else {
            self.dock_area.update(cx, |dock_area, cx| {
                dock_area.add_panel(panel, action.0, None, window, cx);
            });
        }
    }

    /// Kiểm tra center DockItem có TabPanel còn panel nào không.
    fn center_has_no_visible_panel(center: &DockItem, cx: &gpui::App) -> bool {
        match center {
            DockItem::Tabs { view, .. } => view.read(cx).active_panel(cx).is_none(),
            DockItem::Split { items, .. } => !items.iter().any(|item| {
                if let DockItem::Tabs { view, .. } = item {
                    view.read(cx).active_panel(cx).is_some()
                } else {
                    false
                }
            }),
            _ => false,
        }
    }

    /// Action handler: thêm SessionPanel mới vào right dock.
    pub(crate) fn on_action_add_session(
        &mut self,
        _: &AddSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(SessionPanel::new_entity(window, cx));
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(panel, DockPlacement::Right, None, window, cx);
        });
    }

    /// Action handler: thêm SftpPanel mới vào right dock.
    pub(crate) fn on_action_add_sftp_browser(
        &mut self,
        _: &AddSftpBrowser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(SftpPanel::new_entity(window, cx));
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(panel, DockPlacement::Right, None, window, cx);
        });
    }

    /// Action handler: toggle nút dock toggle button (expand/collapse button).
    pub(crate) fn on_action_toggle_dock_toggle_button(
        &mut self,
        _: &ToggleDockToggleButton,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_val = !self.toggle_button_visible.load(Ordering::Relaxed);
        self.toggle_button_visible.store(new_val, Ordering::Relaxed);
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.set_toggle_button_visible(new_val, cx);
        });
    }

    /// Action handler: Quit.
    pub(crate) fn on_action_quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }
}