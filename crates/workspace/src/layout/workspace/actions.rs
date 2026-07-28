//! Action handlers for `OneTermWorkspace`.

use std::sync::atomic::Ordering;

use gpui::{App, Context, Entity, Window};
use gpui_component::dock::{DockArea, DockItem, DockPlacement as UiDockPlacement};
use gpui_component::{WindowExt as _, dialog::DialogButtonProps};

use oneterm_core::DockPlacement;

use oneterm_actions::{
    About, AddPanel, AddPanelWithShell, AddSession, AddSftpBrowser, Find, NewSession, OpenSettings,
    Quit, RightDockMode, SetRightDockMode, ToggleDockToggleButton,
};

impl super::OneTermWorkspace {
    fn to_ui_placement(placement: DockPlacement) -> UiDockPlacement {
        match placement {
            DockPlacement::Center => UiDockPlacement::Center,
            DockPlacement::Left => UiDockPlacement::Left,
            DockPlacement::Bottom => UiDockPlacement::Bottom,
            DockPlacement::Right => UiDockPlacement::Right,
        }
    }

    /// Action handler: add a new TerminalPanel.
    pub(crate) fn on_action_add_panel(
        &mut self,
        action: &AddPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let panel = super::build_named_panel("terminal", &self.dock_area.downgrade(), window, cx);

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
                dock_area.add_panel(panel, Self::to_ui_placement(action.0), None, window, cx);
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
        let Some(cmds) = oneterm_state::commands::commands(cx) else {
            return;
        };
        let panel = (cmds.new_terminal_with_shell)(action.0, window, cx);

        let center_empty = {
            let dock = self.dock_area.read(cx);
            Self::center_has_no_visible_panel(&dock.center(), cx)
        };

        if center_empty {
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
                dock_area.add_panel(panel, UiDockPlacement::Center, None, window, cx);
            });
        }
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
        if let Some(cmds) = oneterm_state::commands::commands(cx) {
            (cmds.open_new_session_dialog)(window, cx);
        }
    }

    /// Action handler: ensure the right dock (which hosts the combined Side
    /// panel) is open. The Side panel always contains the Session section, so
    /// "Add Session" simply reveals the right dock rather than adding a new
    /// panel — the right dock is now a single `DockItem::Panel`, and
    /// `DockArea::add_panel(Right)` is a no-op on `DockItem::Panel`.
    pub(crate) fn on_action_add_session(
        &mut self,
        _: &AddSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        super::set_right_dock_open(&self.dock_area, true, window, cx);
    }

    /// Action handler: ensure the right dock (which hosts the combined Side
    /// panel) is open. The Side panel always contains the SFTP section, so
    /// "Add SFTP Browser" simply reveals the right dock.
    pub(crate) fn on_action_add_sftp_browser(
        &mut self,
        _: &AddSftpBrowser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        super::set_right_dock_open(&self.dock_area, true, window, cx);
    }

    /// Action handler: switch the right dock to the panels for the given
    /// [`RightDockMode`] (SSH Client = Session + SFTP, Agent = Agent panels).
    ///
    /// Rebuilds the right dock as a fresh `DockItem::Panel` of the mode's
    /// registered panel, preserving the dock's current width + open/collapsed
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

        // Persist the new mode to ui_config.json.
        oneterm_settings::UiConfig::global(cx).update(cx, |cfg, _cx| {
            cfg.right_dock_mode = new_mode;
        });
        oneterm_settings::UiConfig::persist(cx);
    }

    /// Apply `mode` to the right dock. For `None` the dock is hidden (collapsed)
    /// without rebuilding its panel, so the previous content is restored when the
    /// user switches back. For `SshClient`/`Agent` the dock is rebuilt as a fresh
    /// `DockItem::Panel` of the mode's registered panel, preserving the dock
    /// width and forcing the dock open. Used by the action handler above and by
    /// the startup apply in `OneTermWorkspace::new`.
    pub(crate) fn switch_right_dock_mode(
        dock_area: &Entity<DockArea>,
        mode: RightDockMode,
        window: &mut Window,
        cx: &mut App,
    ) {
        if mode.is_none() {
            // None mode — hide the right dock without rebuilding its panel, so
            // switching back to SSH Client / Agent restores the previous content.
            super::set_right_dock_open(dock_area, false, window, cx);
            return;
        }
        let weak = dock_area.downgrade();
        let panel = super::build_named_panel(mode.panel_name(), &weak, window, cx);
        let right = DockItem::panel(panel);
        dock_area.update(cx, |view, cx| {
            // Snapshot the current right dock's size so the swap preserves the
            // user's last dock width. Force the dock open — selecting SSH Client /
            // Agent is an explicit request to show the right dock, even if it was
            // collapsed by the dock toggle button or by a previous None selection.
            let right_size = view
                .right_dock()
                .map(|dock| Some(dock.read(cx).size()))
                .unwrap_or(Some(gpui::px(480.)));
            view.set_right_dock(right, right_size, true, window, cx);
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
        if let Some(cmds) = oneterm_state::commands::commands(cx) {
            (cmds.open_settings)(cx);
        }
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
        if let Some(cmds) = oneterm_state::commands::commands(cx) {
            let dock_area = self.dock_area.clone();
            (cmds.find_in_active_terminal)(&dock_area, window, cx);
        }
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
        if let Some(cmds) = oneterm_state::commands::commands(cx) {
            (cmds.open_about)(window, cx);
            return;
        }

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
