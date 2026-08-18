//! The [`TerminalPanel`] type: [`PanelSpec`] + the [`TerminalPanel::open`]
//! constructor, accessors, and the dock [`Panel`]/[`Focusable`]/[`EventEmitter`]
//! trait implementations (including the tab-strip `title()` element).
//!
//! Space operations live in [`super::ops`], the context-menu action handlers +
//! [`Render`](gpui::Render) impl live in [`super::actions`], and tab-title
//! resolution lives in [`super::title`].

use std::sync::Arc;

use gpui::{
    Anchor, App, AppContext as _, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::DropdownMenu as _,
};
use oneterm_core::{LocalShellConfig, SessionDuplicateConfig, ShellKind};
use oneterm_state::AppServices;
use oneterm_terminal::PtySize;
use oneterm_terminal::TerminalSession;

use oneterm_actions::{AddPanelWithShell, NewSession};
use oneterm_settings::TabTitleMode;

use super::super::security::security_policy_from_settings;
use super::super::space::{DragTerminalTab, SpaceId, SpaceTree, SplitContext};
use super::super::view::{LocalTerminalView, TerminalDeps, TerminalViewEvent};

/// Initial PTY size for a freshly spawned session; the element resizes it to
/// the real grid on the first prepaint.
pub(crate) const INITIAL_PTY_SIZE: PtySize = PtySize { rows: 24, cols: 80 };

/// Panel displaying a Terminal Tab (a tree of Spaces).
pub struct TerminalPanel {
    /// DockArea identity used to publish active state only to this workspace.
    pub(super) workspace_id: Option<EntityId>,
    /// The pane tree — leaves are terminals or empty placeholders.
    pub(super) tree: SpaceTree,
    /// Reference to the `TabPanel` containing this panel — used for the close-tab
    /// button and to remove the tab when the last Space closes.
    pub(super) tab_panel: Option<WeakEntity<TabPanel>>,
    /// Whether this panel is the currently selected tab in the `TabPanel`.
    pub(super) is_active: bool,
    /// Tab title fallback — "Terminal" for local, session label for SSH.
    pub(super) tab_title: String,
    /// Manual tab title override selected by the user (wins over OSC 0/2).
    pub(super) tab_title_override: Option<String>,
    /// Subscriptions to every terminal leaf's `TitleChanged` — rebuilt whenever
    /// the set of terminals changes (split / close / drop / fill).
    pub(super) _title_subs: Vec<Subscription>,
    /// Subscription to global `TerminalSettings` changes.
    pub(super) _settings_sub: Subscription,
    /// The services this panel's terminal views receive (ARCH-20).
    pub(super) deps: TerminalDeps,
}

/// What a new terminal tab hosts. Passed to [`TerminalPanel::open`].
pub enum PanelSpec {
    /// The default local shell from settings, bound to the dock area
    /// `workspace` (the primary workspace when `None`).
    DefaultShell { workspace: Option<EntityId> },
    /// A local shell of a specific kind (other shell settings unchanged).
    Shell(ShellKind),
    /// An already-connected session (SSH connect, duplicate) with its tab
    /// title and optional non-secret duplication metadata.
    Session {
        session: Box<dyn TerminalSession>,
        title: String,
        duplicate_config: Option<SessionDuplicateConfig>,
    },
}

impl TerminalPanel {
    /// Create a panel entity for `spec`. The single public constructor.
    pub fn open(spec: PanelSpec, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::from_spec(spec, window, cx))
    }

    /// Build the panel for `spec`: spawn/wrap the session view, put it in a
    /// single-leaf Space tree, focus it, and wire the title + settings
    /// subscriptions. When a local shell fails to spawn the tree starts empty
    /// (the user can retry from the placeholder).
    pub(crate) fn from_spec(spec: PanelSpec, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let primary = oneterm_state::AppState::primary_workspace_id(cx);
        // The one place the terminal feature reads its process globals; the
        // views get them handed down (ARCH-20).
        let deps = TerminalDeps::from_globals(cx);
        let (view, tab_title, workspace_id) = match spec {
            PanelSpec::DefaultShell { workspace } => (
                Self::spawn_local_view(&deps, None, window, cx),
                "Terminal".to_string(),
                workspace.or(primary),
            ),
            PanelSpec::Shell(kind) => (
                Self::spawn_local_view(&deps, Some(kind), window, cx),
                "Terminal".to_string(),
                primary,
            ),
            PanelSpec::Session {
                session,
                title,
                duplicate_config,
            } => {
                let session_entity = cx.new(|_| session);
                let view_deps = deps.clone();
                let view = cx.new(|cx| {
                    let mut view = LocalTerminalView::new(session_entity, view_deps, window, cx);
                    view.duplicate_config = duplicate_config;
                    view
                });
                (Some(view), title, primary)
            }
        };

        let focus = cx.focus_handle();
        let tree = match &view {
            Some(view) => {
                // Focus the terminal view right after creation.
                view.read(cx).focus_handle(cx).focus(window, cx);
                SpaceTree::new_terminal(view.clone(), focus)
            }
            None => {
                log::warn!("TerminalPanel: spawn failed, creating empty tree");
                SpaceTree::new_empty(focus)
            }
        };
        let active = tree.active();

        let _settings_sub = cx.observe(&deps.settings, |_this, _settings, cx| {
            cx.notify();
        });

        let mut this = Self {
            workspace_id,
            tree,
            tab_panel: None,
            is_active: false,
            tab_title,
            tab_title_override: None,
            _title_subs: Vec::new(),
            _settings_sub,
            deps,
        };
        if let Some(view) = &view {
            this.attach_split_ctx(view, active, cx);
        }
        this.rebuild_title_subs(cx);
        this
    }

    /// Spawn a fresh default local session + view.
    /// Returns `None` if the session could not be spawned (e.g. missing shell).
    /// The caller should handle the `None` case by showing an error state.
    pub(super) fn spawn_local_view(
        deps: &TerminalDeps,
        shell_kind_override: Option<ShellKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let shell = {
            let settings = deps.settings.read(cx);
            match shell_kind_override {
                Some(kind) => {
                    // Override the shell kind but keep other settings (utf8, cwd, env, args).
                    let mut shell = settings.shell.clone();
                    shell.kind = kind;
                    // Clear program so resolve_shell auto-detects for the new kind.
                    shell.program = None;
                    shell
                }
                None => settings.shell.clone(),
            }
        };
        Self::spawn_local_view_with_config(deps, shell, window, cx)
    }

    /// Spawn a local terminal view from an exact shell configuration.
    fn spawn_local_view_with_config(
        deps: &TerminalDeps,
        shell: LocalShellConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let (scrollback_history, security) = {
            let settings = deps.settings.read(cx);
            (
                settings.scrollback_history,
                security_policy_from_settings(settings),
            )
        };
        let factory = AppServices::session_factory(cx);
        let duplicate_config = SessionDuplicateConfig::Local(shell.clone());
        let session: Box<dyn TerminalSession> =
            match factory.spawn_local(shell, INITIAL_PTY_SIZE, scrollback_history, security) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to spawn local terminal session: {e}");
                    return None;
                }
            };
        let session_entity = cx.new(|_| session);
        let view_deps = deps.clone();
        Some(cx.new(|cx| {
            let mut view = LocalTerminalView::new(session_entity, view_deps, window, cx);
            view.duplicate_config = Some(duplicate_config);
            view
        }))
    }

    /// Empty Space ids in visual tree order.
    pub(crate) fn empty_space_destinations(&self) -> Vec<SpaceId> {
        self.tree.empty_space_destinations()
    }

    /// Point `view`'s context menu at Space `space_id` in this panel.
    pub(super) fn attach_split_ctx(
        &self,
        view: &Entity<LocalTerminalView>,
        space_id: SpaceId,
        cx: &mut Context<Self>,
    ) {
        let panel = cx.entity().downgrade();
        view.update(cx, |v, _| {
            v.split_ctx = Some(SplitContext { panel, space_id });
        });
    }

    /// (Re)subscribe to `TitleChanged` on every terminal leaf so the tab strip
    /// refreshes on OSC 0/2 title changes regardless of which leaf changed.
    pub(super) fn rebuild_title_subs(&mut self, cx: &mut Context<Self>) {
        let views = self.tree.terminal_views();
        self._title_subs = views
            .into_iter()
            .map(|view| {
                cx.subscribe(&view, |_this, _view, _ev: &TerminalViewEvent, cx| {
                    cx.notify();
                })
            })
            .collect();
    }

    /// The active Space's terminal view (used by Edit ▸ Find). `None` when the
    /// active Space is empty.
    pub(crate) fn active_view(&self) -> Option<Entity<LocalTerminalView>> {
        self.tree.active_terminal()
    }

    /// Session network stats for the active Space (SSH only — `None` for local
    /// or an empty Space). Used by the StatusBar.
    pub(crate) fn network_stats(&self, cx: &App) -> Option<oneterm_terminal::NetStats> {
        self.tree
            .active_terminal()?
            .read(cx)
            .session
            .read(cx)
            .capabilities()
            .network_stats
    }

    /// Breadcrumb label for the active Space's session. `None` when the active
    /// Space is empty or has no cwd yet.
    pub(crate) fn breadcrumb_label(&self, cx: &App) -> Option<String> {
        let view = self.tree.active_terminal()?;
        let s = view.read(cx).session.read(cx);
        let breadcrumb = s.breadcrumb_text();
        let fg = s.foreground_process();
        breadcrumb.map(|bc| {
            if let Some(proc) = fg {
                format!("{} — {}", proc, bc)
            } else {
                bc
            }
        })
    }

    /// Number of Spaces in this tab.
    pub(crate) fn leaf_count(&self) -> usize {
        self.tree.leaf_count()
    }

    /// The resolved tab label (manual override, live OSC 0/2 title, or static
    /// fallback), respecting the `tab_title_mode` setting. Used as the Agent Panel tab-group
    /// title (`docs/agent-panel-display.md` §2.1).
    ///
    /// Reads the active terminal's session title via `v.read(cx)`. Do **not**
    /// call this from inside a `LocalTerminalView::update` closure on the active
    /// terminal — it would re-enter the view's lease and panic
    /// (`entity_map::read` double-lease). Use [`Self::tab_label_with_title`]
    /// instead, passing the title fetched from the already-leased view's own
    /// `session.read(cx).title()`.
    pub(crate) fn tab_label(&self, cx: &App) -> String {
        let mode = self.deps.settings.read(cx).tab_title_mode;
        let session_title = self
            .tree
            .active_terminal()
            .and_then(|v| v.read(cx).session.read(cx).title());
        let live = match mode {
            TabTitleMode::Osc => session_title.as_deref(),
            TabTitleMode::Default => None,
        };
        self.effective_tab_label(live)
    }

    /// Same as [`Self::tab_label`] but takes the live session title as a
    /// parameter instead of reading the active terminal view. Use this from
    /// contexts where the active `LocalTerminalView` is already being updated
    /// (e.g. `push_agent_status` inside the view's `update` closure) — reading
    /// it again would re-enter the view's lease and panic.
    ///
    /// `live_title` is the OSC 0/2 title of the terminal the event came from
    /// (the caller already has the view and can read its own session). It is
    /// only used when `tab_title_mode == Osc` and no manual override exists; in
    /// `Default` mode the static `tab_title` fallback is returned.
    pub(crate) fn tab_label_with_title(&self, live_title: Option<&str>, cx: &App) -> String {
        let mode = self.deps.settings.read(cx).tab_title_mode;
        let live = match mode {
            TabTitleMode::Osc => live_title,
            TabTitleMode::Default => None,
        };
        self.effective_tab_label(live)
    }

    /// The Agent Panel ordering key for `space_id`: its 0-based depth-first
    /// (left-to-right) position in the current tree. User-facing labels use the
    /// stable `SpaceId` instead.
    pub(crate) fn space_order(&self, space_id: SpaceId) -> usize {
        self.tree.leaf_index(space_id).unwrap_or(0)
    }

    /// A weak handle to the containing `TabPanel` (for Agent Panel click-to-focus).
    pub(crate) fn tab_panel_weak(&self) -> Option<WeakEntity<TabPanel>> {
        self.tab_panel.clone()
    }

    /// Whether this tab has no terminal Spaces left (all empty).
    pub(crate) fn has_no_terminals(&self) -> bool {
        self.tree.has_no_terminals()
    }

    /// Shut down all terminal sessions and cancel all tasks.
    /// Called by `Panel::on_removed`, last-space close, and error paths.
    /// Idempotent.
    pub(crate) fn shutdown(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        for view in self.tree.terminal_views() {
            view.update(cx, |v, cx| v.shutdown(cx));
        }
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to the active Space — terminal view's handle, or the empty
        // placeholder's handle.
        //
        // Invariant: the tree always holds ≥ 1 leaf and `active` always points at
        // an existing leaf (`SpaceTree::set_active`/`close` re-point it on
        // removal), so `active_focus_handle` is always `Some`.
        self.tree
            .active_focus_handle(cx)
            .expect("active Space always has a focus handle")
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }

    fn on_removed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Close all sessions and cancel all tasks when the panel is removed
        // from the TabPanel (tab close button, middle-click, drag removal).
        self.shutdown(window, cx);
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_panel = self.tab_panel.clone();
        let panel_entity = cx.entity().clone();
        let panel_weak = cx.entity().downgrade();
        let theme = cx.theme().muted_foreground;
        let highlight = cx.theme().table_active_border;
        let is_active = self.is_active;
        let tab_label = self.tab_label(cx);
        let drag_title: SharedString = tab_label.clone().into();
        let rename_title = tab_label.clone();
        let title_label_id = SharedString::from(format!("tab-title-label-{:?}", panel_entity));

        h_flex()
            .id("tab-title")
            .relative()
            .h_full()
            .w_full()
            .min_w(px(100.))
            .items_center()
            .gap_1()
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(-px(20.))
                        .right(-px(20.))
                        .h(px(2.))
                        .bg(highlight),
                )
            })
            .mr(-px(5.))
            // Drag the tab into an empty Space (our own payload — the dock's
            // native `DragPanel` is `pub(crate)` and unusable here).
            .when_some(tab_panel.clone(), |this, tpw| {
                this.on_drag(
                    DragTerminalTab {
                        panel: panel_weak.clone(),
                        tab_panel: tpw,
                        title: drag_title.clone(),
                    },
                    |drag, _pos, _win, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    },
                )
            })
            // Middle-click on a tab → close that tab.
            .on_mouse_down(MouseButton::Middle, {
                let tp = tab_panel.clone();
                let pe = panel_entity.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(tp) = tp.as_ref().and_then(|tp| tp.upgrade()) {
                        let panel: Arc<dyn PanelView> = Arc::new(pe.clone());
                        tp.update(cx, |tp, cx| {
                            tp.remove_panel(panel, window, cx);
                        });
                    }
                }
            })
            .child(
                super::title::tab_title_label()
                    .id(title_label_id)
                    .on_click({
                        let panel = cx.entity().clone();
                        let rename_title = rename_title.clone();
                        move |event, window, cx| {
                            if event.click_count() != 2 {
                                return;
                            }
                            cx.stop_propagation();
                            super::title::open_tab_title_dialog(
                                panel.clone(),
                                rename_title.clone(),
                                window,
                                cx,
                            );
                        }
                    })
                    .child(tab_label),
            )
            .when_some(tab_panel, |this, tp| {
                this.child(
                    div()
                        .id("tab-close")
                        .flex_shrink_0()
                        .cursor_pointer()
                        .size_4()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .hover(move |this| this.bg(theme.opacity(0.15)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            if let Some(tp) = tp.upgrade() {
                                let panel: Arc<dyn PanelView> = Arc::new(panel_entity.clone());
                                tp.update(cx, |tp, cx| {
                                    tp.remove_panel(panel, window, cx);
                                });
                            }
                        })
                        .child(Icon::new(IconName::Close).xsmall().text_color(theme)),
                )
            })
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    /// "+" button next to the zoom button — dropdown to spawn a new terminal tab
    /// with a specific shell, or open the New SSH Session dialog.
    fn title_suffix(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let btn = Button::new("add-shell-tab")
            .icon(IconName::Plus)
            .xsmall()
            .ghost()
            .tab_stop(false)
            .tooltip("New Terminal")
            .dropdown_menu(|menu, _, _| {
                let mut menu = menu;
                // Platform-specific shells.
                #[cfg(windows)]
                {
                    menu = menu
                        .menu(
                            "Command Prompt",
                            Box::new(AddPanelWithShell(ShellKind::Cmd)),
                        )
                        .menu(
                            "PowerShell",
                            Box::new(AddPanelWithShell(ShellKind::PowerShell)),
                        )
                        .menu("PowerShell 7", Box::new(AddPanelWithShell(ShellKind::Pwsh)));
                }
                #[cfg(not(windows))]
                {
                    menu = menu
                        .menu("Bash", Box::new(AddPanelWithShell(ShellKind::Bash)))
                        .menu("Sh", Box::new(AddPanelWithShell(ShellKind::Sh)))
                        .menu("Zsh", Box::new(AddPanelWithShell(ShellKind::Zsh)));
                }
                menu.separator()
                    .menu("New SSH Session", Box::new(NewSession))
            })
            .anchor(Anchor::TopRight);
        Some(btn)
    }

    fn on_added_to(
        &mut self,
        tab_panel: WeakEntity<TabPanel>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.tab_panel = Some(tab_panel);
    }

    fn set_active(&mut self, active: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }
        if active {
            self.publish_active_session(cx);
        }
    }
}
