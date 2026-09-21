//! [`OneTermWorkspace`] — OneTerm's main view.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    Styled, Task, Window, div,
};
use gpui_component::Root;
use gpui_component::dock::{DockArea, DockSkin, PanelStyle};

use oneterm_state::AppState;

use crate::layout::{statusbar, title_bar::AppTitleBar};
use crate::widgets::{StatusText, breadcrumb, datetime_clock, git_status, net_speed, resource};

pub(crate) mod actions;
mod dock_skin;
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

/// Share of the window the right dock may take. The terminal is the primary
/// surface and keeps the majority at laptop widths (`US-0113`): at 900 px the
/// dock stops at ~315 px instead of the ~490 px it used to hold.
const RIGHT_DOCK_MAX_SHARE: f32 = 0.35;

/// Floor for the ceiling above. On a window so narrow that a third of it is not
/// a usable panel, the dock keeps this much rather than collapsing — a dock the
/// user did not close stays on screen, and the splitter is still theirs to drag.
const MIN_RIGHT_DOCK_WIDTH: gpui::Pixels = gpui::px(240.);

/// The persisted layout this process may start from, or `None` for the fixed
/// default layout.
///
/// M1 (`DEC-0019`, `IN-0043` MAJ-2): an elevated window **does not read
/// `docks.json` at all**. `load_layout` restores the side docks *by name*, so a
/// saved `ssh_client` or `agent` dock would be built before any gate on the
/// layout builders could decline to build one — and declining to call
/// `set_dock` does not remove a dock that already exists. Gating the builders
/// was necessary and not sufficient: the restore is the path every user who has
/// run OneTerm once actually takes, so the "default" path in practice was the
/// broken one.
///
/// `read` is a closure so the test can prove the stronger claim — that the
/// document is not read, rather than merely not used.
fn startup_dock_document(
    restricted: bool,
    read: impl FnOnce() -> Option<oneterm_state::dock_persistence::DockDocument>,
) -> Option<oneterm_state::dock_persistence::DockDocument> {
    if restricted {
        log::info!("elevated window: not reading docks.json; using the default layout");
        return None;
    }
    read()
}

/// The width the right dock may actually take in a window this wide.
///
/// A ceiling, not a proportion: a requested width that already fits is returned
/// untouched, so the user's own dragged width wins inside the allowed range.
/// Only a width that would leave the terminal with less than its share is
/// capped. A window of unknown width (before the first layout pass) constrains
/// nothing.
pub(crate) fn clamp_right_dock_width(
    requested: gpui::Pixels,
    window_width: gpui::Pixels,
) -> gpui::Pixels {
    if window_width <= gpui::px(0.) {
        return requested;
    }
    let ceiling = (window_width * RIGHT_DOCK_MAX_SHARE).max(MIN_RIGHT_DOCK_WIDTH);
    requested.min(ceiling)
}

/// Put the user's preferred (unclamped) right-dock width into a dumped layout.
///
/// `docks.json` stores the **preference**; the clamp decides what is applied at
/// the current window size. Writing the applied width instead would ratchet the
/// preference down: one session at a narrow window and the wide dock is gone for
/// good (`US-0113`).
pub(crate) fn state_with_preferred_width(
    mut state: gpui_component::dock::DockAreaState,
    preferred: gpui::Pixels,
) -> gpui_component::dock::DockAreaState {
    use gpui_component::dock::DockState;

    if let Some(right) = state.right_dock.as_ref() {
        state.right_dock = Some(DockState::new(
            right.panel().clone(),
            right.placement(),
            preferred,
            right.open(),
        ));
    }
    state
}

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
) -> Option<std::sync::Arc<dyn gpui_component::dock::BasePanelView>> {
    use gpui_component::dock::{PanelBuildContext, PanelInfo, PanelRegistry, PanelState};
    // A leaf panel carries no layout payload (CORR-64) — `PanelInfo::tabs`
    // would describe a tab container, which this is not.
    let state = PanelState {
        panel_name: name.to_string(),
        children: Vec::new(),
        info: PanelInfo::panel(serde_json::Value::Null),
    };
    let context = PanelBuildContext::new(dock_area.clone(), &state, &state.info);
    let panel = PanelRegistry::build_panel(name, context, window, cx);
    if panel.is_none() {
        log::error!("build_named_panel: panel {name:?} is not registered with PanelRegistry");
    }
    panel
}

pub use oneterm_state::dock_persistence::state_file;

/// Main workspace: title bar + dock area + status bar.
pub struct OneTermWorkspace {
    pub title_bar: Entity<AppTitleBar>,
    pub dock_area: Entity<DockArea>,
    _dock_skin: Rc<DockSkin>,
    /// Datetime clock — created once so the 1s timer fires reliably.
    pub clock: Entity<StatusText>,
    /// Network speed indicator — created once so the 1s timer fires reliably.
    pub net_speed: Entity<StatusText>,
    /// Breadcrumb (cwd + foreground process) indicator — created once so the
    /// 500ms timer fires reliably.
    pub breadcrumb: Entity<StatusText>,
    /// Git status of the active local terminal's cwd — created once so the
    /// 500ms timer fires reliably.
    pub git_status: Entity<StatusText>,
    /// CPU/memory resource indicator — created once so the 2s timer fires reliably.
    pub resource: Entity<StatusText>,
    last_layout_state: Option<gpui_component::dock::DockAreaState>,
    _save_layout_task: Option<Task<()>>,

    /// The right-dock width the user last asked for, before any clamping. The
    /// applied width is `clamp_right_dock_width` of this against the current
    /// window, so narrowing the window narrows the dock and widening it brings
    /// the user's width back (`US-0113`).
    preferred_right_dock_width: gpui::Pixels,

    /// Name of the panel currently zoomed (fullscreen), mirrored into docks.json.
    zoomed_panel: Option<String>,
    /// Set once the exit-time layout write has run, so the two exit hooks
    /// (`on_app_quit` while the window is still open, `on_release` when the
    /// window closes) do not write the same document twice.
    layout_saved_on_exit: bool,
}

impl OneTermWorkspace {
    /// Create a new workspace: load the old layout (keep right dock + settings),
    /// but reset the center (terminal tabs) to a single default tab.
    ///
    /// Precondition: the composition root has initialised the shared globals
    /// (`AppState`, `UiConfig`, `AppServices`) before the window opens.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // An elevated window is a deliberately smaller application: local shells
        // only, marked, and writing no configuration (`DEC-0019` M1/M4/M5). Read
        // from the process token, never from a launch argument — and true as
        // well when the token query itself failed, because a process that cannot
        // prove it is ordinary is treated as though it were not.
        let elevated = oneterm_core::elevation::is_restricted();
        let (dock_area, dock_skin) =
            dock_skin::dock_area(MAIN_DOCK_ID, Some(MAIN_DOCK_VERSION), window, cx);
        dock_skin.set_panel_style(PanelStyle::TabBar, cx);
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
        //
        // M1 (`DEC-0019`, `IN-0043` MAJ-2): an elevated window does not read it
        // at all. `load_layout` restores the **side docks by name**, so a saved
        // `ssh_client` or `agent` dock would be built here — before any gate on
        // the layout builders can decline to build one — and then left in place,
        // because declining to call `set_dock(Right, ..)` does not remove a dock
        // that is already there. Gating the builders was necessary and not
        // sufficient: the restore is the path every user who has run OneTerm
        // once actually takes. An elevated window therefore gets the fixed
        // default layout, every time, with no saved zoom and no saved width.
        let document = startup_dock_document(elevated, || {
            persistence::read_dock_document().unwrap_or_else(|error| {
                log::warn!("{error}; using the default layout");
                None
            })
        });
        let saved_zoom = document
            .as_ref()
            .and_then(|document| document.zoomed_panel.clone());

        let loaded = document
            .and_then(|document| persistence::load_layout(&dock_area, &document, window, cx).ok())
            .is_some();

        // The width the saved layout carries is the user's preference; the
        // builders below apply whatever this window has room for.
        let preferred_right_dock_width = dock_area
            .read(cx)
            .dock_size(gpui_component::dock::DockPlacement::Right)
            .unwrap_or(DEFAULT_RIGHT_DOCK_WIDTH);

        if loaded {
            layout::reset_center_only(weak_dock_area, preferred_right_dock_width, window, cx);
        } else {
            layout::reset_default_layout(weak_dock_area, window, cx);
        }

        // Apply the persisted right-dock mode. Both layout builders above set the
        // right dock to the SSH Client `ssh_client_panel`; if the user last chose
        // Agent Mode or None, apply that now (Agent swaps the panel, None hides
        // the dock). Preserves the dock width; for Agent it keeps the open state
        // just loaded, for None it collapses the dock. An elevated window has no
        // right dock to apply a mode to, and `switch_right_dock_mode` refuses
        // there anyway (M1).
        let saved_mode = oneterm_settings::UiConfig::global(cx)
            .read(cx)
            .right_dock_mode;
        if saved_mode != oneterm_actions::RightDockMode::SshClient {
            Self::switch_right_dock_mode(&dock_area, saved_mode, window, cx);
        }

        cx.observe_in(&dock_area, window, move |this, dock_area, window, cx| {
            let zoomed_panel = dock_area
                .read(cx)
                .zoomed_group()
                .and_then(|node| {
                    oneterm_state::dock_util::panel_name_for_tab_node(dock_area.read(cx), node, cx)
                })
                .map(str::to_owned);
            if this.zoomed_panel != zoomed_panel {
                this.zoomed_panel = zoomed_panel;
            }
            this.sync_right_dock_mode(&dock_area, cx);
            this.track_preferred_right_dock_width(&dock_area, window, cx);
            this.save_layout(&dock_area, window, cx);
        })
        .detach();

        // The dock keeps an absolute width, so only a window resize can make it
        // outgrow its share of the window (`US-0113`).
        cx.observe_window_bounds(window, |this, window, cx| {
            this.apply_right_dock_width(window, cx);
        })
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
            // M5: the same string the OS title bar carries, from one source, so
            // the two markers cannot disagree.
            // The plain application name only: the app menu bar renders this, and
            // the kit gives a menu name no place for a second colour. The
            // elevation suffix is drawn beside it by `AppTitleBar::render`, which
            // is ours to colour (`DEC-0019` M5 as amended).
            let bar = AppTitleBar::new(
                oneterm_core::elevation::window_title_parts(oneterm_core::elevation::elevation()).0,
                window,
                cx,
            );
            if elevated {
                // M1: an elevated window has no right dock, so there is nothing
                // to switch between — the three segments are absent rather than
                // disabled.
                bar
            } else {
                bar.child(|_window, cx| crate::layout::title_bar::mode_toggle_group(cx))
            }
        });

        let clock = datetime_clock(window, cx);
        let net_speed = net_speed(dock_area.downgrade(), window, cx);
        // The status bar refreshes both budgets every frame; they start wide
        // enough that the first frame shows the labels whole.
        let breadcrumb = breadcrumb(
            dock_area.downgrade(),
            Rc::new(std::cell::Cell::new(gpui::px(f32::MAX))),
            window,
            cx,
        );
        let git_status = git_status(
            dock_area.downgrade(),
            Rc::new(std::cell::Cell::new(gpui::px(f32::MAX))),
            window,
            cx,
        );
        let resource = resource(window, cx);

        let me = Self {
            title_bar,
            dock_area: dock_area.clone(),
            _dock_skin: dock_skin,
            clock,
            net_speed,
            breadcrumb,
            git_status,
            resource,
            last_layout_state: None,
            _save_layout_task: None,
            preferred_right_dock_width,
            zoomed_panel: None,
            layout_saved_on_exit: false,
        };

        // Restore zoom (fullscreen) for the panel matching the saved name.
        if let Some(name) = saved_zoom {
            me.restore_zoom(&name, window, cx);
        }

        me
    }

    /// Keep `UiConfig.right_dock_mode` telling the truth about the right dock.
    ///
    /// The tab bar's dock button and the status bar's dock button toggle the
    /// dock through the kit, without going through `SetRightDockMode`. Without
    /// this the title bar's segmented control kept claiming "SSH Client" over a
    /// collapsed dock (`BUG-0067`).
    fn sync_right_dock_mode(&mut self, dock_area: &Entity<DockArea>, cx: &mut Context<Self>) {
        let state = {
            let area = dock_area.read(cx);
            if !area.has_dock(gpui_component::dock::DockPlacement::Right) {
                return;
            }
            let open = area.is_dock_open(gpui_component::dock::DockPlacement::Right);
            let shows_agent =
                right_dock_panel_mode(area, cx) == Some(oneterm_actions::RightDockMode::Agent);
            (open, shows_agent)
        };
        let current = oneterm_settings::UiConfig::global(cx)
            .read(cx)
            .right_dock_mode;
        let next = right_dock_mode_for(state.0, current, state.1);
        if next == current {
            return;
        }
        oneterm_settings::UiConfig::global(cx).update(cx, |config, cx| {
            config.right_dock_mode = next;
            cx.notify();
        });
        self.title_bar.update(cx, |_, cx| cx.notify());
        oneterm_settings::UiConfig::persist(cx);
    }

    /// Remember a width the user set themselves.
    ///
    /// The dock area reports one width; anything other than the one this
    /// workspace would have applied came from the splitter, and that is the
    /// width to come back to once the window has room for it again.
    fn track_preferred_right_dock_width(
        &mut self,
        dock_area: &Entity<DockArea>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_right_dock_width = Self::absorb_dragged_right_dock_width(
            dock_area,
            self.preferred_right_dock_width,
            window,
            cx,
        );
    }

    /// Take a width the dock reports as the user's own, and hold it to the
    /// ceiling. Returns the preference to remember.
    ///
    /// A width equal to the one this workspace would have applied is its own
    /// doing and changes nothing. Any other width came from the splitter: it
    /// becomes the preference — so the dock returns to it once the window has
    /// room — and a drag past the ceiling is capped **here**, rather than left
    /// standing until the next window resize snaps it back.
    pub(crate) fn absorb_dragged_right_dock_width(
        dock_area: &Entity<DockArea>,
        preferred: gpui::Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Pixels {
        use gpui_component::dock::DockPlacement;

        let window_width = window.viewport_size().width;
        let Some(size) = dock_area.read(cx).dock_size(DockPlacement::Right) else {
            return preferred;
        };
        if size == clamp_right_dock_width(preferred, window_width) {
            return preferred;
        }
        let capped = clamp_right_dock_width(size, window_width);
        if capped != size {
            dock_area.update(cx, |dock_area, cx| {
                dock_area.set_dock_size(DockPlacement::Right, capped, window, cx);
            });
        }
        size
    }

    /// Re-apply the preferred width against the window's current size.
    ///
    /// Called on a window resize, and on a drag that went past the ceiling —
    /// never per frame, so a drag inside the allowed range is never fought.
    fn apply_right_dock_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_component::dock::DockPlacement;

        let width = clamp_right_dock_width(
            self.preferred_right_dock_width,
            window.viewport_size().width,
        );
        self.dock_area.clone().update(cx, |dock_area, cx| {
            if dock_area.has_dock(DockPlacement::Right)
                && dock_area.dock_size(DockPlacement::Right) != Some(width)
            {
                dock_area.set_dock_size(DockPlacement::Right, width, window, cx);
            }
        });
    }

    /// Write the current layout synchronously at exit; the first hook to run wins.
    fn save_layout_on_exit(&mut self, trigger: &str, cx: &App) {
        if self.layout_saved_on_exit {
            return;
        }
        self.layout_saved_on_exit = true;
        let state = state_with_preferred_width(
            self.dock_area.read(cx).dump(cx),
            self.preferred_right_dock_width,
        );
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
                let state = state_with_preferred_width(
                    dock_area.read(cx).dump(cx),
                    this.preferred_right_dock_width,
                );
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

    /// Restore zoom for the tab group whose active panel matches `name`.
    fn restore_zoom(&self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if restore_zoom_in_dock(&self.dock_area, name, window, cx) {
            log::info!("Restored zoom for panel \"{name}\"");
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

/// The mode the right dock's panel currently **is**, whatever `UiConfig` says.
///
/// Scoped to the right dock on purpose: a whole-tree search answers for the
/// centre first, and would name Agent mode for an Agent panel that had been
/// dragged out of the dock.
pub(crate) fn right_dock_panel_mode(
    dock_area: &DockArea,
    cx: &App,
) -> Option<oneterm_actions::RightDockMode> {
    use oneterm_actions::RightDockMode;
    use oneterm_state::panel_names;

    let tree = dock_area.layout(gpui_component::dock::DockPlacement::Right)?;
    let panel = tree.panels().next().and_then(|id| dock_area.panel(id))?;
    match panel.panel_name(cx) {
        panel_names::SSH_CLIENT => Some(RightDockMode::SshClient),
        panel_names::AGENT => Some(RightDockMode::Agent),
        _ => None,
    }
}

/// The mode the title bar must show for a right dock in this state.
///
/// The dock's open state wins: a collapsed dock is `None` whatever was
/// persisted, and a dock the user reopened with a dock button takes the mode of
/// the panel that came back. A dock that is open while a real mode is persisted
/// keeps it — this never rebuilds a panel, it only renames what is on screen.
pub(crate) fn right_dock_mode_for(
    open: bool,
    current: oneterm_actions::RightDockMode,
    shows_agent: bool,
) -> oneterm_actions::RightDockMode {
    use oneterm_actions::RightDockMode;
    match (open, current) {
        (false, _) => RightDockMode::None,
        (true, RightDockMode::None) if shows_agent => RightDockMode::Agent,
        (true, RightDockMode::None) => RightDockMode::SshClient,
        (true, mode) => mode,
    }
}

/// Restore zoom for the tab group whose active panel matches `name`.
fn restore_zoom_in_dock(
    dock_area: &Entity<DockArea>,
    name: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let target =
        oneterm_state::dock_util::find_tab_node_by_panel_name(dock_area.read(cx), name, cx);
    let Some(node) = target else {
        return false;
    };
    dock_area.update(cx, |dock_area, cx| {
        dock_area.set_zoomed_in(node, window, cx)
    });
    dock_area.read(cx).zoomed_group() == Some(node)
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
            .child(statusbar::build_status_bar(self, window, cx))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}
