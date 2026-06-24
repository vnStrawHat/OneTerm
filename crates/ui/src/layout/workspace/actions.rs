//! Action handlers cho `MyTermWorkspace`.

use std::sync::Arc;

use gpui::{Context, Window};
use gpui_component::dock::DockPlacement;

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
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel(panel, action.0, None, window, cx);
        });
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

    /// Action handler: toggle nút dock toggle button.
    pub(crate) fn on_action_toggle_dock_toggle_button(
        &mut self,
        _: &ToggleDockToggleButton,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_button_visible = !self.toggle_button_visible;
        self.dock_area.update(cx, |dock_area, cx| {
            dock_area.set_toggle_button_visible(self.toggle_button_visible, cx);
        });
    }

    /// Action handler: Quit.
    pub(crate) fn on_action_quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }
}
