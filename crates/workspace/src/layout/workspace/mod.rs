//! [`OneTermWorkspace`] — OneTerm's main view.
//!
//! The original `workspace.rs` module has been split into `workspace/`.

use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EntityId, InteractiveElement as _, IntoElement,
    ParentElement, Render, Styled, Task, Window, div,
};
use gpui_component::Root;
use gpui_component::dock::{DockArea, DockEvent, PanelEvent, ToggleZoom};

use oneterm_state::AppState;

use crate::layout::{statusbar, title_bar::AppTitleBar};
use crate::widgets::{BreadcrumbIndicator, DateTimeClock, NetSpeedIndicator, ResourceIndicator};

pub(crate) mod actions;
pub(crate) mod layout;
pub(crate) mod persistence;
pub(crate) mod zoom;

/// Save the dock state to `docks.json` — used when the window closes (`on_release`).
/// Reads `dock_area`, `zoomed_panel`, and `toggle_button_visible` from the global AppState.
///
/// The write is performed synchronously on the calling (UI) thread. This is a
/// deliberate exception to the "persist off the UI thread" rule: the caller
/// invokes `cx.quit()` right after, and gpui's `App::shutdown` only awaits
/// `on_app_quit` observers — a detached background task is not awaited and can
/// be killed by process exit before the layout reaches disk (CORR-04). The
/// document is small and the process is shutting down, so a blocking atomic
/// write is acceptable here.
pub fn save_dock_state_on_close(cx: &App) {
    let (weak_dock, zoomed_name, tbv) = {
        let state = AppState::global(cx).read(cx);
        (
            state.dock_area.clone(),
            state
                .zoomed_panel
                .as_ref()
                .and_then(|m| m.lock().ok())
                .and_then(|g| g.clone()),
            state
                .toggle_button_visible
                .as_ref()
                .map(|a| a.load(Ordering::Relaxed))
                .unwrap_or(true),
        )
    };
    let Some(weak_dock) = weak_dock else {
        return;
    };
    let Some(dock_area) = weak_dock.upgrade() else {
        log::warn!("save_dock_state_on_close: dock_area already dropped");
        return;
    };
    let dock_state = dock_area.read(cx).dump(cx);
    log::info!("save_dock_state_on_close → writing dock state before quit");
    persistence::save_state_logged(&dock_state, zoomed_name.as_deref(), tbv, "on_close");
}

pub const MAIN_DOCK_VERSION: usize = 3;
pub const MAIN_DOCK_ID: &str = "main-dock";

/// Set the Right Dock open/closed (no-op if there is no right dock).
///
/// Used by action handlers that reveal the right dock (Add Session, Add SFTP
/// Browser, mode toggle).
///
/// Generic over a [`gpui::AppContext`] so it can be called from both the
/// workspace action handler and a terminal panel hook. The `window` is passed
/// explicitly (same pattern as `DockArea::toggle_dock`) so `Dock::set_open` can
/// defer its collapse on the right window.
pub(crate) use oneterm_state::dock_util::set_right_dock_open;

/// Construct a fresh feature panel by its registered name, via the gpui-component
/// `PanelRegistry`. Each feature crate registers its constructor at init, so the
/// shell can build the default layout and honor `AddPanel`/`AddSession`/
/// `AddSftpBrowser` without depending on the concrete panel types.
pub(crate) fn build_named_panel(
    name: &str,
    dock_area: &gpui::WeakEntity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> std::sync::Arc<dyn gpui_component::dock::PanelView> {
    use gpui_component::dock::{PanelInfo, PanelRegistry, PanelState};
    let state = PanelState {
        panel_name: name.to_string(),
        children: Vec::new(),
        info: PanelInfo::tabs(0),
    };
    std::sync::Arc::from(PanelRegistry::build_panel(
        name,
        dock_area.clone(),
        &state,
        &state.info,
        window,
        cx,
    ))
}

/// Shared path to the persisted dock document.
pub use oneterm_state::paths::state_file;

/// Main workspace: title bar + dock area + status bar.
pub struct OneTermWorkspace {
    pub title_bar: Entity<AppTitleBar>,
    pub dock_area: Entity<DockArea>,
    /// Datetime clock — created once so the 1s timer fires reliably.
    pub clock: Entity<DateTimeClock>,
    /// Network speed indicator — created once so the 1s timer fires reliably.
    pub net_speed: Entity<NetSpeedIndicator>,
    /// Breadcrumb (cwd + foreground process) indicator — created once so the
    /// 500ms timer fires reliably.
    pub breadcrumb: Entity<BreadcrumbIndicator>,
    /// CPU/memory resource indicator — created once so the 2s timer fires reliably.
    pub resource: Entity<ResourceIndicator>,
    last_layout_state: Option<gpui_component::dock::DockAreaState>,
    toggle_button_visible: Arc<AtomicBool>,
    _save_layout_task: Option<Task<()>>,

    /// Mirror of the zoom state: name of the panel currently zoomed (fullscreen).
    /// `gpui-component` keeps `TabPanel.zoomed` (private) + `DockArea.zoom_view`
    /// (private), so they cannot be read from outside the crate → track it ourselves
    /// via the `PanelEvent::ZoomIn`/`ZoomOut` subscription. Uses `Arc<Mutex<..>>`
    /// shared with the `on_app_quit` closure to read safely even after the workspace
    /// entity has been dropped during shutdown.
    zoomed_panel: Arc<Mutex<Option<String>>>,
    /// `TabPanel`s already subscribed — avoids duplicate subscriptions (LayoutChanged
    /// fires multiple times, and TabPanels can be recreated).
    subscribed_tabs: HashSet<EntityId>,
}

impl OneTermWorkspace {
    /// Create a new workspace: load the old layout (keep right dock + settings),
    /// but reset the center (terminal tabs) to a single default tab.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        AppState::init(cx);

        let dock_area = cx.new(|cx| {
            use gpui_component::dock::PanelStyle;
            DockArea::new(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx)
                .panel_style(PanelStyle::TabBar)
        });
        let weak_dock_area = dock_area.downgrade();

        // Save WeakEntity<DockArea> into AppState — the SSH connect dialog uses it
        // to add a terminal tab after a successful connection.
        AppState::global(cx).update(cx, |s, cx| {
            s.dock_area.get_or_insert_with(|| weak_dock_area.clone());
            s.register_workspace(&weak_dock_area);
            cx.notify();
        });

        // Read the name of the zoomed panel BEFORE the layout is reset (the center
        // always resets, and reset_* rewrites docks.json without the zoom → must save first).
        let saved_zoom = persistence::read_zoomed_panel();
        // Read toggle_button_visible BEFORE reset_* rewrites docks.json.
        let saved_toggle_button_visible = persistence::read_toggle_button_visible().unwrap_or(true);

        match Self::load_layout(dock_area.clone(), window, cx) {
            Ok(()) => {
                layout::reset_center_only(weak_dock_area, saved_toggle_button_visible, window, cx);
            }
            Err(_) => {
                layout::reset_default_layout(weak_dock_area, window, cx);
            }
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

        // Mirror toggle_button_visible — `Arc<AtomicBool>` shared between
        // subscription callbacks and the `on_app_quit` closure.
        let toggle_button_visible = Arc::new(AtomicBool::new(saved_toggle_button_visible));

        // Apply the saved toggle_button_visible to the DockArea (default = true).
        if !saved_toggle_button_visible {
            dock_area.update(cx, |dock_area, cx| {
                dock_area.set_toggle_button_visible(false, cx);
            });
        }

        // Mirror the zoom state (name of the zoomed panel) — `Arc<Mutex<..>>` shared
        // between subscription callbacks and the `on_app_quit` closure to read safely
        // even after the workspace entity has been dropped during shutdown.
        let zoomed_panel = Arc::new(Mutex::new(None::<String>));

        // Save zoomed_panel + toggle_button_visible into AppState — shared with the
        // `on_release` callback in `window.rs` to save the dock state on close.
        AppState::global(cx).update(cx, |s, cx| {
            s.zoomed_panel = Some(zoomed_panel.clone());
            s.toggle_button_visible = Some(toggle_button_visible.clone());
            cx.notify();
        });

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

        // Fallback: on_app_quit may fire in some cases (e.g. cx.shutdown), but is usually
        // dropped with the entity when the window closes (see `save_dock_state_on_close`).
        cx.on_app_quit({
            let dock_area = dock_area.clone();
            let zoomed_panel = zoomed_panel.clone();
            let toggle_button_visible = toggle_button_visible.clone();
            move |_, cx| {
                let state = dock_area.read(cx).dump(cx);
                let zoomed_name = zoomed_panel.lock().ok().and_then(|g| g.clone());
                let tbv = toggle_button_visible.load(Ordering::Relaxed);
                cx.background_executor().spawn(async move {
                    persistence::save_state_logged(
                        &state,
                        zoomed_name.as_deref(),
                        tbv,
                        "on_app_quit",
                    );
                })
            }
        })
        .detach();

        let title_bar = cx.new(|cx| {
            AppTitleBar::new("OneTerm", window, cx)
                .child(|_window, cx| crate::layout::title_bar::mode_toggle_group(cx))
        });

        let clock = DateTimeClock::new_entity(window, cx);
        let net_speed = NetSpeedIndicator::new_entity(dock_area.downgrade(), window, cx);
        let breadcrumb = BreadcrumbIndicator::new_entity(dock_area.downgrade(), window, cx);
        let resource = ResourceIndicator::new_entity(window, cx);

        let mut me = Self {
            title_bar,
            dock_area: dock_area.clone(),
            clock,
            net_speed,
            breadcrumb,
            resource,
            last_layout_state: None,
            toggle_button_visible,
            _save_layout_task: None,
            zoomed_panel,
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

    /// Debounce the save by 5s, skip when the state is unchanged.
    fn save_layout(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dock_area = dock_area.clone();
        // Snapshot the zoomed panel name at schedule time (mirror state).
        let zoomed_name = self.zoomed_panel.lock().ok().and_then(|g| g.clone());
        self._save_layout_task = Some(cx.spawn_in(window, async move |story, window| {
            window
                .background_executor()
                .timer(Duration::from_secs(2))
                .await;

            _ = story.update_in(window, move |this, _, cx| {
                let state = dock_area.read(cx).dump(cx);
                if Some(&state) == this.last_layout_state.as_ref() {
                    return;
                }
                let tbv = this.toggle_button_visible.load(Ordering::Relaxed);
                this.last_layout_state = Some(state.clone());
                cx.background_executor()
                    .spawn(async move {
                        persistence::save_state_logged(
                            &state,
                            zoomed_name.as_deref(),
                            tbv,
                            "debounce",
                        );
                    })
                    .detach();
            });
        }));
    }

    /// Subscribe to `PanelEvent` on every not-yet-subscribed `TabPanel` — updates the
    /// `zoomed_panel` mirror. Called after each `DockEvent::LayoutChanged` and at init.
    fn sync_tab_subscriptions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = zoom::collect_tab_panels(&self.dock_area.read(cx), cx);
        log::info!("sync_tab_subscriptions → found {} tab panel(s)", tabs.len());
        for tp in tabs {
            let id = tp.entity_id();
            if self.subscribed_tabs.insert(id) {
                let zoomed_panel = self.zoomed_panel.clone();
                let dock_area = self.dock_area.clone();
                let toggle_button_visible = self.toggle_button_visible.clone();
                cx.subscribe_in(
                    &tp,
                    window,
                    move |_this, tp, ev: &PanelEvent, _window, cx| match ev {
                        PanelEvent::ZoomIn => {
                            // Resolve the active panel's name at zoom time.
                            let name = tp
                                .read(cx)
                                .active_panel(cx)
                                .map(|p| p.panel_name(cx).to_string());
                            log::info!("PanelEvent::ZoomIn → name={name:?}");
                            if let Ok(mut g) = zoomed_panel.lock() {
                                *g = name.clone();
                            }
                            // Save to docks.json IMMEDIATELY — independent of quit/debounce.
                            let state = dock_area.read(cx).dump(cx);
                            let tbv = toggle_button_visible.load(Ordering::Relaxed);
                            cx.background_executor()
                                .spawn(async move {
                                    persistence::save_state_logged(
                                        &state,
                                        name.as_deref(),
                                        tbv,
                                        "zoom_in",
                                    );
                                })
                                .detach();
                            cx.notify();
                        }
                        PanelEvent::ZoomOut => {
                            log::info!("PanelEvent::ZoomOut");
                            if let Ok(mut g) = zoomed_panel.lock() {
                                *g = None;
                            }
                            let state = dock_area.read(cx).dump(cx);
                            let tbv = toggle_button_visible.load(Ordering::Relaxed);
                            cx.background_executor()
                                .spawn(async move {
                                    persistence::save_state_logged(&state, None, tbv, "zoom_out");
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
        let target = zoom::find_tab_by_panel_name(&self.dock_area.read(cx), name, cx);
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
    /// This keeps the shell free of any `views::settings` dependency.
    pub fn bind_keys(cx: &mut App) {
        if let Some(cmds) = oneterm_state::commands::commands(cx) {
            (cmds.setup_key_bindings)(cx);
        }
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
            .on_action(cx.listener(Self::on_action_add_session))
            .on_action(cx.listener(Self::on_action_add_sftp_browser))
            .on_action(cx.listener(Self::on_action_set_right_dock_mode))
            .on_action(cx.listener(Self::on_action_add_panel_with_shell))
            .on_action(cx.listener(Self::on_action_new_session))
            .on_action(cx.listener(Self::on_action_toggle_dock_toggle_button))
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
