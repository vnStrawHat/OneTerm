//! Action handlers for `OneTermWorkspace`.

use std::sync::{Arc, atomic::Ordering};

use gpui::{Context, Window};
use gpui_component::{
    WindowExt as _,
    dialog::DialogButtonProps,
    dock::{DockItem, DockPlacement},
};

use crate::{
    actions::{About, AddPanel, AddSession, AddSftpBrowser, Quit, ToggleDockToggleButton},
    views::{SessionPanel, SftpPanel, TerminalPanel},
};

impl super::OneTermWorkspace {
    /// Action handler: add a new TerminalPanel.
    pub(crate) fn on_action_add_panel(
        &mut self,
        action: &AddPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: Arc<dyn gpui_component::dock::PanelView> =
            Arc::new(TerminalPanel::new_entity(window, cx));

        // When all tabs are closed, the center DockItem still keeps the old entry but
        // the inner TabPanel has no panels left → add_panel would add to a "ghost"
        // TabPanel that is not rendered. Detect this and recreate the center.
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

    /// Check whether the center DockItem has any TabPanel with panels left.
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

    /// Action handler: add a new SessionPanel to the right dock.
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

    /// Action handler: add a new SftpPanel to the right dock.
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

    /// Action handler: toggle the dock toggle button (expand/collapse button).
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

    /// Action handler: About — open the About dialog.
    ///
    /// Triggered by OneTerm ▸ About and Help ▸ About OneTerm in the AppMenuBar.
    pub(crate) fn on_action_about(
        &mut self,
        _: &About,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.open_alert_dialog(cx, |alert, _, _| {
            alert
                .title("About OneTerm")
                .description(format!(
                    "OneTerm v{}\n\nA terminal application for local and SSH sessions.\n\
                     Built with GPUI + alacritty_terminal.",
                    env!("ONETERM_VERSION")
                ))
                .button_props(DialogButtonProps::default().ok_text("Close"))
        });
    }
}
