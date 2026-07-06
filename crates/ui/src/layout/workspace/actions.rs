//! Action handlers for `OneTermWorkspace`.

use std::sync::{Arc, atomic::Ordering};

use gpui::{Context, Window};
use gpui_component::{
    WindowExt as _,
    dialog::DialogButtonProps,
    dock::{DockItem, DockPlacement},
};

use crate::{
    actions::{
        About, AddPanel, AddSession, AddSftpBrowser, Find, OpenSettings, Quit,
        ToggleAutoHideRightDock, ToggleDockToggleButton,
    },
    state::{AppState, TerminalSettings},
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

    /// Action handler: toggle the "Auto-hide Right Dock on Local Shell" setting.
    ///
    /// Flips `TerminalSettings::auto_hide_right_dock_on_local` and immediately
    /// applies the rule based on the currently active tab (`AppState::active_is_local`):
    ///   - enabled  → open the right dock for SSH tabs, close it for local shells.
    ///   - disabled → restore the right dock (open).
    pub(crate) fn on_action_toggle_auto_hide_right_dock(
        &mut self,
        _: &ToggleAutoHideRightDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_val = !TerminalSettings::global(cx)
            .read(cx)
            .auto_hide_right_dock_on_local;
        TerminalSettings::global(cx).update(cx, |st, cx| {
            st.auto_hide_right_dock_on_local = new_val;
            cx.notify();
        });

        let is_local = AppState::global(cx).read(cx).active_is_local;
        let want_open = if new_val { !is_local } else { true };
        // Persist to terminal.json (load → mutate only this field → save) so the
        // preference survives restarts; other fields in the file are preserved.
        let mut cfg = crate::state::terminal_config::TerminalConfig::load();
        cfg.layout.auto_hide_right_dock_on_local = new_val;
        if let Err(e) = cfg.save() {
            log::warn!("Failed to persist terminal.json: {e}");
        }

        super::set_right_dock_open(&self.dock_area, want_open, window, cx);
        cx.refresh_windows();
    }

    /// Action handler: open the General Settings UI in a separate window.
    ///
    /// The settings window is a standalone `WindowHandle<Root>` wrapping a
    /// [`SettingsPanel`] (see [`crate::views::settings::open_settings_window`]).
    /// Closing it does not quit the app — only the main window's `on_release`
    /// hook does that.
    pub(crate) fn on_action_open_settings(
        &mut self,
        _: &OpenSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::views::settings::open_settings_window(cx).detach();
    }

    /// Action handler: Quit.
    pub(crate) fn on_action_quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    /// Action handler: Find — activate the in-terminal search bar on the
    /// active terminal panel.
    ///
    /// Triggered by Edit ▸ Find in the AppMenuBar. Walks the DockArea to find
    /// the active terminal panel, then calls `open_search` on its
    /// `LocalTerminalView`. If the search bar is already open, toggles it
    /// closed (same behavior as Ctrl+F).
    pub(crate) fn on_action_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        let dock_area = self.dock_area.clone();
        let tab_panels = super::zoom::collect_tab_panels(dock_area.read(cx), cx);
        for tp in tab_panels {
            if let Some(panel) = tp.read(cx).active_panel(cx) {
                if panel.panel_name(cx) == "terminal" {
                    let any_view = panel.view();
                    if let Ok(entity) = any_view.downcast::<TerminalPanel>() {
                        entity.update(cx, |tp, cx| {
                            tp.view().update(cx, |v, cx| {
                                if v.search_active {
                                    v.close_search(cx);
                                } else {
                                    v.open_search(window, cx);
                                }
                            });
                        });
                        return;
                    }
                }
            }
        }
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
