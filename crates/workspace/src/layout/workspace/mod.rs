//! [`OneTermWorkspace`] — OneTerm's main view.

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EntityId, InteractiveElement as _, IntoElement,
    ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::Root;
use gpui_component::dock::{DockArea, DockEvent, PanelEvent, ToggleZoom};

use oneterm_state::AppState;

use crate::layout::{statusbar, title_bar::AppTitleBar};
use crate::widgets::{StatusText, breadcrumb, datetime_clock, net_speed, resource};

pub(crate) mod actions;
pub(crate) mod layout;
pub(crate) mod persistence;

#[cfg(test)]
mod layout_tests;
#[cfg(test)]
pub(crate) mod test_panels;

pub const MAIN_DOCK_VERSION: usize = 3;
pub const MAIN_DOCK_ID: &str = "main-dock";

/// Right-dock width used when no saved layout provides one (first launch, or
/// a layout without a right dock).
pub(crate) const DEFAULT_RIGHT_DOCK_WIDTH: gpui::Pixels = gpui::px(480.);

pub(crate) use oneterm_state::dock_util::set_right_dock_open;

/// Construct a fresh feature panel by its registered name, via the gpui-component
/// `PanelRegistry`. Each feature crate registers its constructor at init, so the
/// shell can build the default layout and honor `AddPanel` without depending
/// on the concrete panel types.
///
/// Names come from [`oneterm_state::panel_names`]. When `name` is not
/// registered (stale saved layout, or a feature whose `init()` did not run)
/// gpui-component substitutes a placeholder `InvalidPanel`; that case is
/// logged at error level here so it never fails silently.
pub(crate) fn build_named_panel(
    name: &str,
    dock_area: &gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> std::sync::Arc<dyn gpui_component::dock::PanelView> {
    use gpui_component::dock::{PanelInfo, PanelRegistry, PanelState};
    // A leaf panel carries no layout payload (CORR-64) — `PanelInfo::tabs`
    // would describe a tab container, which this is not.
    let state = PanelState {
        panel_name: name.to_string(),
        children: Vec::new(),
        info: PanelInfo::panel(serde_json::Value::Null),
    };
    let panel: std::sync::Arc<dyn gpui_component::dock::PanelView> = std::sync::Arc::from(
        PanelRegistry::build_panel(name, dock_area.clone(), &state, &state.info, window, cx),
    );
    // `PanelRegistry` does not expose its registered names, but an unregistered
    // name always yields gpui-component's `InvalidPanel` placeholder.
    if panel.panel_name(cx) == INVALID_PANEL_NAME && name != INVALID_PANEL_NAME {
        log::error!(
            "build_named_panel: panel {name:?} is not registered with PanelRegistry; \
             rendering placeholder"
        );
    }
    panel
}

/// `Panel::panel_name` of gpui-component's placeholder for unregistered names
/// (`dock/invalid_panel.rs`).
const INVALID_PANEL_NAME: &str = "InvalidPanel";

pub use oneterm_state::paths::state_file;

/// Main workspace: title bar + dock area + status bar.
pub struct OneTermWorkspace {
    pub title_bar: Entity<AppTitleBar>,
    pub dock_area: Entity<DockArea>,
    /// Datetime clock — created once so the 1s timer fires reliably.
    pub clock: Entity<StatusText>,
    /// Network speed indicator — created once so the 1s timer fires reliably.
    pub net_speed: Entity<StatusText>,
    /// Breadcrumb (cwd + foreground process) indicator — created once so the
    /// 500ms timer fires reliably.
    pub breadcrumb: Entity<StatusText>,
    /// CPU/memory resource indicator — created once so the 2s timer fires reliably.
    pub resource: Entity<StatusText>,
    last_layout_state: Option<gpui_component::dock::DockAreaState>,
    _save_layout_task: Option<Task<()>>,

    /// Mirror of the zoom state: name of the panel currently zoomed (fullscreen).
    /// `gpui-component` keeps `TabPanel.zoomed` (private) + `DockArea.zoom_view`
    /// (private), so they cannot be read from outside the crate → track it ourselves
    /// via the `PanelEvent::ZoomIn`/`ZoomOut` subscription.
    zoomed_panel: Option<String>,
    /// Set once the exit-time layout write has run, so the two exit hooks
    /// (`on_app_quit` while the window is still open, `on_release` when the
    /// window closes) do not write the same document twice.
    layout_saved_on_exit: bool,
    /// `TabPanel`s already subscribed — avoids duplicate subscriptions (LayoutChanged
    /// fires multiple times, and TabPanels can be recreated).
    subscribed_tabs: HashSet<EntityId>,
}

/// Forget `TabPanel`s that no longer exist in the dock (their subscriptions
/// ended with the entity) so the subscribed set cannot grow with every tab
/// open/close (CORR-22).
fn retain_live_tabs(subscribed: &mut HashSet<EntityId>, live: impl IntoIterator<Item = EntityId>) {
    let live: HashSet<EntityId> = live.into_iter().collect();
    subscribed.retain(|id| live.contains(id));
}

impl OneTermWorkspace {
    /// Create a new workspace: load the old layout (keep right dock + settings),
    /// but reset the center (terminal tabs) to a single default tab.
    ///
    /// Precondition: the composition root has initialised the shared globals
    /// (`AppState`, `UiConfig`, `AppServices`) before the window opens.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dock_area = cx.new(|cx| {
            use gpui_component::dock::PanelStyle;
            DockArea::new(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx)
                .panel_style(PanelStyle::TabBar)
        });
        let weak_dock_area = dock_area.downgrade();

        // Register the DockArea as the primary workspace in AppState — feature
        // crates (e.g. the SSH connect dialog) place panels through it.
        AppState::global(cx).update(cx, |s, cx| {
            s.register_workspace(&weak_dock_area);
            cx.notify();
        });

        // Read docks.json once (PERF-27): the zoomed panel name must be taken
        // BEFORE the layout is reset (the center always resets, and reset_*
        // rewrites docks.json without the zoom), and the same document feeds
        // the layout load. An unreadable file is left to the first save:
        // `dock_persistence` — the document owner — quarantines it there.
        let document = persistence::read_dock_document().unwrap_or_else(|error| {
            log::warn!("{error}; using the default layout");
            None
        });
        let saved_zoom = document
            .as_ref()
            .and_then(|document| document.zoomed_panel.clone());

        let loaded = document
            .and_then(|document| persistence::load_layout(&dock_area, &document, window, cx).ok())
            .is_some();
        if loaded {
            layout::reset_center_only(weak_dock_area, window, cx);
        } else {
            layout::reset_default_layout(weak_dock_area, window, cx);
        }

        // Apply the persisted right-dock mode. Both layout builders above set the
        // right dock to the SSH Client `ssh_client_panel`; if the user last chose
        // Agent Mode or None, apply that now (Agent swaps the panel, None hides
        // the dock). Preserves the dock width; for Agent it keeps the open state
        // just loaded, for None it collapses the dock.
        let saved_mode = oneterm_settings::UiConfig::global(cx)
            .read(cx)
            .right_dock_mode;
        if saved_mode != oneterm_actions::RightDockMode::SshClient {
            Self::switch_right_dock_mode(&dock_area, saved_mode, window, cx);
        }

        cx.subscribe_in(
            &dock_area,
            window,
            move |this, dock_area, ev: &DockEvent, window, cx| match ev {
                DockEvent::LayoutChanged => {
                    // Subscribe every new TabPanel (dynamically added tab/panel).
                    this.sync_tab_subscriptions(window, cx);
                    this.save_layout(dock_area, window, cx);
                }
                _ => {}
            },
        )
        .detach();

        // Exit-time layout persistence (CORR-04). Both hooks write synchronously
        // on the UI thread — a deliberate exception to the "persist off the UI
        // thread" rule: gpui's `App::shutdown` awaits `on_app_quit` futures for
        // at most 200 ms and never awaits detached background tasks, so a
        // background write could be killed by process exit before the layout
        // reaches disk. The document is small and the process is exiting.
        //
        // - `on_app_quit` runs while the window is still open (Quit action /
        //   Cmd+Q): the workspace entity is alive, so the write happens here.
        // - `on_release` runs when the window is closed by the user: the root
        //   view is dropped before `cx.quit()`, so `on_app_quit` would find the
        //   entity gone. The release hook still owns `self.dock_area` and
        //   writes the final layout before the shell quits.
        cx.on_app_quit(|this, cx| {
            this.save_layout_on_exit("on_app_quit", cx);
            async {}
        })
        .detach();
        cx.on_release(|this, cx| this.save_layout_on_exit("on_close", cx))
            .detach();

        let title_bar = cx.new(|cx| {
            AppTitleBar::new("OneTerm", window, cx)
                .child(|_window, cx| crate::layout::title_bar::mode_toggle_group(cx))
        });

        let clock = datetime_clock(window, cx);
        let net_speed = net_speed(dock_area.downgrade(), window, cx);
        let breadcrumb = breadcrumb(dock_area.downgrade(), window, cx);
        let resource = resource(window, cx);

        let mut me = Self {
            title_bar,
            dock_area: dock_area.clone(),
            clock,
            net_speed,
            breadcrumb,
            resource,
            last_layout_state: None,
            _save_layout_task: None,
            zoomed_panel: None,
            layout_saved_on_exit: false,
            subscribed_tabs: HashSet::new(),
        };

        // Observe dock entities (left/right/bottom) for resize — Dock::resize()
        // calls cx.notify() which fires observers, but does NOT emit
        // DockEvent::LayoutChanged. Without this, dock width is lost on restart.
        {
            let (right, left, bottom) = {
                let docks = dock_area.read(cx);
                (
                    docks.right_dock().cloned(),
                    docks.left_dock().cloned(),
                    docks.bottom_dock().cloned(),
                )
            };
            for dock in right.into_iter().chain(left).chain(bottom) {
                let da = dock_area.clone();
                cx.observe_in(&dock, window, move |this, _, window, cx| {
                    this.save_layout(&da, window, cx);
                })
                .detach();
            }
        }

        // Subscribe every existing TabPanel (loaded from docks.json / created in reset_*).
        me.sync_tab_subscriptions(window, cx);

        // Restore zoom (fullscreen) for the panel matching the saved name.
        if let Some(name) = saved_zoom {
            me.restore_zoom(&name, window, cx);
        }

        me
    }

    /// Write the current layout synchronously at exit; the first hook to run wins.
    fn save_layout_on_exit(&mut self, trigger: &str, cx: &App) {
        if self.layout_saved_on_exit {
            return;
        }
        self.layout_saved_on_exit = true;
        let state = self.dock_area.read(cx).dump(cx);
        log::info!("save_layout_on_exit [trigger={trigger}] → writing dock state before quit");
        persistence::save_state_logged(&state, self.zoomed_panel.as_deref(), trigger);
    }

    /// Debounce the save by 2s, skip when the state is unchanged.
    fn save_layout(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock_area = dock_area.clone();
        self._save_layout_task = Some(cx.spawn_in(window, async move |this, window| {
            window
                .background_executor()
                .timer(Duration::from_secs(2))
                .await;

            // The workspace may be gone before the debounce elapses; the exit
            // hooks own the final write in that case.
            _ = this.update_in(window, move |this, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if Some(&state) == this.last_layout_state.as_ref() {
                    return;
                }
                this.last_layout_state = Some(state.clone());
                let zoomed_name = this.zoomed_panel.clone();
                cx.background_executor()
                    .spawn(async move {
                        persistence::save_state_logged(&state, zoomed_name.as_deref(), "debounce");
                    })
                    .detach();
            });
        }));
    }

    /// Subscribe to `PanelEvent` on every not-yet-subscribed `TabPanel` — updates the
    /// `zoomed_panel` mirror. Called after each `DockEvent::LayoutChanged` and at init.
    fn sync_tab_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = oneterm_state::dock_util::collect_tab_panels(&self.dock_area.read(cx), cx);
        log::debug!("sync_tab_subscriptions → found {} tab panel(s)", tabs.len());
        retain_live_tabs(
            &mut self.subscribed_tabs,
            tabs.iter().map(|tp| tp.entity_id()),
        );
        for tp in tabs {
            let id = tp.entity_id();
            if self.subscribed_tabs.insert(id) {
                cx.subscribe_in(
                    &tp,
                    window,
                    move |this, tp, ev: &PanelEvent, _window, cx| match ev {
                        PanelEvent::ZoomIn => {
                            // Resolve the active panel's name at zoom time.
                            let name = tp
                                .read(cx)
                                .active_panel(cx)
                                .map(|p| p.panel_name(cx).to_string());
                            log::debug!("PanelEvent::ZoomIn → name={name:?}");
                            this.zoomed_panel = name.clone();
                            // Save to docks.json IMMEDIATELY — independent of quit/debounce.
                            let state = this.dock_area.read(cx).dump(cx);
                            cx.background_executor()
                                .spawn(async move {
                                    persistence::save_state_logged(
                                        &state,
                                        name.as_deref(),
                                        "zoom_in",
                                    );
                                })
                                .detach();
                            cx.notify();
                        }
                        PanelEvent::ZoomOut => {
                            log::debug!("PanelEvent::ZoomOut");
                            this.zoomed_panel = None;
                            let state = this.dock_area.read(cx).dump(cx);
                            cx.background_executor()
                                .spawn(async move {
                                    persistence::save_state_logged(&state, None, "zoom_out");
                                })
                                .detach();
                            cx.notify();
                        }
                        _ => {}
                    },
                )
                .detach();
            }
        }
    }

    /// Restore zoom: find the `TabPanel` whose active panel matches `name` → focus +
    /// dispatch `ToggleZoom` (through gpui-component's proper code path → consistent
    /// toolbar state, `TabPanel.zoomed` set correctly).
    fn restore_zoom(&self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let target =
            oneterm_state::dock_util::find_tab_by_panel_name(&self.dock_area.read(cx), name, cx);
        if let Some(tp) = target {
            if let Some(panel) = tp.read(cx).active_panel(cx) {
                panel.focus_handle(cx).focus(window, cx);
                window.dispatch_action(Box::new(ToggleZoom), cx);
                log::info!("Restored zoom for panel \"{name}\"");
            }
        }
    }

    /// Bind global key bindings for the workspace.
    ///
    /// Delegates to the settings feature (via the workspace command registry),
    /// which snapshots the current bindings then applies OneTerm's overrides.
    /// This keeps the shell free of any settings-UI dependency.
    pub fn bind_keys(cx: &mut App) {
        (oneterm_state::commands::commands(cx).setup_key_bindings)(cx);
    }
}

impl Render for OneTermWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .id("oneterm-workspace")
            .on_action(cx.listener(Self::on_action_add_panel))
            .on_action(cx.listener(Self::on_action_set_right_dock_mode))
            .on_action(cx.listener(Self::on_action_add_panel_with_shell))
            .on_action(cx.listener(Self::on_action_new_session))
            .on_action(cx.listener(Self::on_action_quit))
            .on_action(cx.listener(Self::on_action_find))
            .on_action(cx.listener(Self::on_action_about))
            .on_action(cx.listener(Self::on_action_open_settings))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(self.title_bar.clone())
            .child(div().flex_1().min_h_0().child(self.dock_area.clone()))
            .child(statusbar::build_status_bar(
                &self.dock_area,
                self.clock.clone(),
                self.net_speed.clone(),
                self.breadcrumb.clone(),
                self.resource.clone(),
                window,
                cx,
            ))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
