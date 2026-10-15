//! Action handlers for `OneTermWorkspace`.

use gpui::{Context, Window};
use gpui_component::dock::{
    BasePanelView, DockArea, DockEvent, DockLayout, DockPlacement as UiDockPlacement, PanelHandle,
};

use oneterm_state::commands::commands;
use oneterm_state::panel_names;

use oneterm_actions::{
    About, AddPanel, AddPanelWithShell, Find, NewSession, OpenSettings, Quit, ToggleRightDock,
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
        // M1: the "+" menu's Quick Connect row is absent in an elevated window,
        // but the *action* is bound to `ctrl-shift-n` globally, so the row being
        // gone was never enough (`IN-0043` MAJ-3).
        if !oneterm_actions::action_allowed_when_elevated("new_ssh_session") {
            return;
        }
        (commands(cx).open_quick_connect_dialog)(window, cx);
    }

    /// Action handler: open or close the right dock (`Ctrl+Shift+B` by
    /// default). The dock-area observer records the result as the choice for
    /// the active tab's kind (`IN-0054`), exactly as for a dock button. A no-op
    /// in an elevated window, which has no right dock.
    pub(crate) fn on_action_toggle_right_dock(
        &mut self,
        _: &ToggleRightDock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock_area.update(cx, |area, cx| {
            if area.has_dock(UiDockPlacement::Right) {
                area.toggle_dock(UiDockPlacement::Right, window, cx);
                // `toggle_dock` emits no LayoutChanged; ask for the save.
                cx.emit(DockEvent::LayoutChanged);
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
