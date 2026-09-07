//! Action handlers for `OneTermWorkspace`.

use gpui::{App, Context, Entity, Window};
use gpui_component::dock::{
    BasePanelView, DockArea, DockLayout, DockPlacement as UiDockPlacement, PanelHandle,
};

use oneterm_state::commands::commands;
use oneterm_state::panel_names;

use oneterm_actions::{
    About, AddPanel, AddPanelWithShell, Find, NewSession, OpenSettings, Quit, RightDockMode,
    SetRightDockMode,
};

impl super::OneTermWorkspace {
    /// Add `panel` to the center dock area.
    ///
    /// A normalized empty center has no tab group to receive the panel, so it is
    /// recreated; otherwise the panel joins the existing first tab group.
    fn place_center_panel(
        &mut self,
        panel: std::sync::Arc<dyn gpui_component::dock::PanelView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel: std::sync::Arc<dyn BasePanelView> =
            std::sync::Arc::new(PanelHandle::from_view(panel));
        let center_empty = Self::center_has_no_visible_panel(self.dock_area.read(cx), cx);

        self.dock_area.update(cx, |dock_area, cx| {
            if center_empty {
                let center =
                    DockLayout::v_split().child(DockLayout::tabs().panel_view(panel, cx), None);
                dock_area.set_center(center, window, cx);
            } else {
                dock_area.add_panel_view(panel, UiDockPlacement::Center, None, window, cx);
            }
        });
    }

    /// Action handler: add a new TerminalPanel.
    pub(crate) fn on_action_add_panel(
        &mut self,
        _: &AddPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = super::build_named_panel(
            panel_names::TERMINAL,
            &self.dock_area.downgrade(),
            window,
            cx,
        ) else {
            return;
        };
        self.dock_area.update(cx, |dock_area, cx| {
            if dock_area.is_empty(UiDockPlacement::Center, cx) {
                dock_area.set_center(
                    DockLayout::v_split().child(DockLayout::tabs().panel_view(panel, cx), None),
                    window,
                    cx,
                );
            } else {
                dock_area.add_panel_view(panel, UiDockPlacement::Center, None, window, cx);
            }
        });
    }

    /// Check whether the normalized center tree has any visible panel.
    pub(crate) fn center_has_no_visible_panel(dock_area: &DockArea, cx: &gpui::App) -> bool {
        dock_area.is_empty(UiDockPlacement::Center, cx)
    }

    /// Action handler: add a new TerminalPanel with a specific shell kind.
    ///
    /// Same as `on_action_add_panel` but spawns the terminal with the given
    /// `ShellKind` instead of the default from settings.
    pub(crate) fn on_action_add_panel_with_shell(
        &mut self,
        action: &AddPanelWithShell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = (commands(cx).new_terminal_with_shell)(action.0, window, cx);
        self.place_center_panel(panel, window, cx);
    }

    /// Action handler: open the "New SSH Session" dialog.
    ///
    /// Opens the session creation dialog at the workspace level so it works
    /// even when no SessionPanel is open.
    pub(crate) fn on_action_new_session(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        (commands(cx).open_new_session_dialog)(window, cx);
    }

    /// Action handler: ensure the right dock (which hosts the combined Side
    /// panel) is open. The Side panel always contains the Session section, so
    /// Action handler: switch the right dock to the panels for the given
    /// [`RightDockMode`] (SSH Client = Session + SFTP, Agent = Agent panels).
    ///
    /// Rebuilds the right dock as a single-tab `DockLayout` containing the
    /// mode's registered panel, preserving the dock's current width + open/collapsed
    /// state. Persists the choice to `ui_config.json` so it survives restarts.
    ///
    /// Dispatched by the title bar mode toggle group. No-op if the right dock
    /// is already showing that mode (read from `UiConfig`, the source of truth).
    pub(crate) fn on_action_set_right_dock_mode(
        &mut self,
        action: &SetRightDockMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_mode = action.0;
        let current = oneterm_settings::UiConfig::global(cx)
            .read(cx)
            .right_dock_mode;
        if current == new_mode {
            // Same mode clicked — still apply the requested visibility: hide for
            // None, force-open for SSH Client / Agent (the click is explicit).
            super::set_right_dock_open(&self.dock_area, !new_mode.is_none(), window, cx);
            return;
        }
        Self::switch_right_dock_mode(&self.dock_area, new_mode, window, cx);

        // Persist the new mode to ui_config.json and notify the title bar so the
        // segmented control re-renders before the next click.
        oneterm_settings::UiConfig::global(cx).update(cx, |cfg, cx| {
            cfg.right_dock_mode = new_mode;
            cx.notify();
        });
        self.title_bar.update(cx, |_, cx| cx.notify());
        oneterm_settings::UiConfig::persist(cx);
    }

    /// Apply `mode` to the right dock. For `None` the dock is hidden (collapsed)
    /// without rebuilding its panel, so the previous content is restored when the
    /// user switches back. For `SshClient`/`Agent` the dock is rebuilt as a
    /// single-tab `DockLayout` of the mode's registered panel, preserving the dock
    /// width and forcing the dock open. Used by the action handler above and by
    /// the startup apply in `OneTermWorkspace::new`.
    pub(crate) fn switch_right_dock_mode(
        dock_area: &Entity<DockArea>,
        mode: RightDockMode,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(panel_name) = panel_names::right_dock_panel_name(mode) else {
            // None mode — hide the right dock without rebuilding its panel, so
            // switching back to SSH Client / Agent restores the previous content.
            super::set_right_dock_open(dock_area, false, window, cx);
            return;
        };
        let weak = dock_area.downgrade();
        let Some(panel) = super::build_named_panel(panel_name, &weak, window, cx) else {
            return;
        };
        let right = DockLayout::tabs().panel_view(panel, cx);
        dock_area.update(cx, |view, cx| {
            // Snapshot the current right dock's size so the swap preserves the
            // user's last dock width. Force the dock open — selecting SSH Client /
            // Agent is an explicit request to show the right dock.
            let right_size = view
                .dock_size(UiDockPlacement::Right)
                .unwrap_or(super::DEFAULT_RIGHT_DOCK_WIDTH);
            view.set_dock(UiDockPlacement::Right, right, window, cx);
            view.set_dock_size(UiDockPlacement::Right, right_size, window, cx);
            if !view.is_dock_open(UiDockPlacement::Right) {
                view.toggle_dock(UiDockPlacement::Right, window, cx);
            }
        });
    }

    /// Action handler: open the General Settings UI in a separate window.
    ///
    /// The settings feature owns the window (`WorkspaceCommands::open_settings`).
    /// Closing it does not quit the app — only the main window's `on_release`
    /// hook does that.
    pub(crate) fn on_action_open_settings(
        &mut self,
        _: &OpenSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        (commands(cx).open_settings)(cx);
    }

    /// Action handler: Quit.
    pub(crate) fn on_action_quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    /// Action handler: Find — activate the in-terminal search bar on the
    /// active terminal panel.
    ///
    /// Triggered by the Find key binding (e.g. Ctrl+F). Walks the DockArea to find
    /// the active terminal panel, then calls `open_search` on its
    /// `LocalTerminalView`. If the search bar is already open, toggles it
    /// closed (same behavior as Ctrl+F).
    pub(crate) fn on_action_find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        let dock_area = self.dock_area.clone();
        (commands(cx).find_in_active_terminal)(&dock_area, window, cx);
    }

    /// Action handler: About — open the About dialog.
    ///
    /// Triggered by OneTerm ▸ About in the AppMenuBar.
    pub(crate) fn on_action_about(
        &mut self,
        _: &About,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        (commands(cx).open_about)(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, TestAppContext};
    use gpui_component::dock::{DockArea, DockLayout};

    use super::super::OneTermWorkspace;
    use super::super::test_panels::NamedPanel;

    #[gpui::test]
    fn center_empty_check_follows_normalized_layout(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let (_dock_area, _cx) = cx.add_window_view(|window, cx| {
            let dock_area = cx.new(|cx| DockArea::new("center-test", None, window, cx));
            assert!(OneTermWorkspace::center_has_no_visible_panel(
                dock_area.read(cx),
                cx,
            ));

            let panel = NamedPanel::view("blank", cx);
            dock_area.update(cx, |dock_area, cx| {
                dock_area.set_center(
                    DockLayout::v_split().child(DockLayout::tabs().panel_view(panel, cx), None),
                    window,
                    cx,
                );
            });
            assert!(!OneTermWorkspace::center_has_no_visible_panel(
                dock_area.read(cx),
                cx,
            ));

            DockArea::new("root", None, window, cx)
        });
    }
}
