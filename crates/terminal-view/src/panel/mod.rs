//! [`TerminalPanel`] — a Terminal Tab hosting a tree of resizable **Spaces**.
//!
//! A panel used to wrap exactly one `LocalTerminalView`; it now owns a
//! [`SpaceTree`] whose leaves are terminals or empty placeholders. A tree with a
//! single leaf renders exactly like the old single-terminal panel. See
//! `docs/terminal-split/`.
//!
//! Space operations live in [`ops`] and the context-menu action handlers +
//! [`Render`] impl live in [`actions`].

#[cfg(test)]
mod tests;

mod actions;
mod ops;
mod title;

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
use oneterm_core::ShellKind;
use oneterm_terminal::PtySize;
use oneterm_terminal::TerminalSession;

use oneterm_actions::{AddPanelWithShell, NewSession};
use oneterm_settings::{TabTitleMode, TerminalSettings};

use super::space::{DragTerminalTab, SpaceId, SpaceTree, SplitContext};
use super::view::{LocalTerminalView, TerminalViewEvent};

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
}

/// Resolve the tab label from the live OSC 0/2 title and the static fallback.
pub(crate) fn resolve_tab_label(live: Option<&str>, fallback: &str) -> String {
    match live.filter(|s| !s.is_empty()).map(trim_path_title) {
        Some(t) => t.to_string(),
        None => fallback.to_string(),
    }
}

/// Shorten a title that is just an absolute path to its last path component.
fn trim_path_title(title: &str) -> &str {
    let t = title.trim();
    let bytes = t.as_bytes();
    let is_abs = t.starts_with('/')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'));
    if !is_abs {
        return title;
    }
    match t.rsplit(|c| c == '\\' || c == '/').next() {
        Some(last) if !last.is_empty() => last,
        _ => title,
    }
}

impl TerminalPanel {
    /// Create a panel + spawn the default local session (cmd on Windows).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace_id = oneterm_state::AppState::primary_workspace_id(cx);
        Self::new_internal(None, workspace_id, window, cx)
    }

    /// Create a panel bound to a specific dock/workspace.
    pub fn new_in_workspace(
        workspace_id: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(None, Some(workspace_id), window, cx)
    }

    /// Create a panel + spawn a local session with the given shell kind.
    pub fn new_with_shell(kind: ShellKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace_id = oneterm_state::AppState::primary_workspace_id(cx);
        Self::new_internal(Some(kind), workspace_id, window, cx)
    }

    fn new_internal(
        shell_kind_override: Option<ShellKind>,
        workspace_id: Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = Self::spawn_local_view(shell_kind_override, window, cx);
        let focus = cx.focus_handle();
        let (tree, active) = match view {
            Some(ref view) => {
                let tree = SpaceTree::new_terminal(view.clone(), focus);
                let active = tree.active();
                // Focus the terminal view right after creation.
                view.read(cx).focus_handle(cx).focus(window, cx);
                (tree, active)
            }
            None => {
                // Spawn failed — create an empty tree.
                // The user will see an empty terminal and can retry.
                log::warn!("TerminalPanel::new: spawn failed, creating empty tree");
                let tree = SpaceTree::new_empty(focus.clone());
                let active = tree.active();
                (tree, active)
            }
        };

        let _settings_sub = cx.observe(&TerminalSettings::global(cx), |_this, _settings, cx| {
            cx.notify();
        });

        // Focus is handled inside the match above (only when spawn succeeds).

        let mut this = Self {
            workspace_id,
            tree,
            tab_panel: None,
            is_active: false,
            tab_title: "Terminal".to_string(),
            tab_title_override: None,
            _title_subs: Vec::new(),
            _settings_sub,
        };
        if let Some(view) = &view {
            this.attach_split_ctx(view, active, cx);
        }
        this.rebuild_title_subs(cx);
        this
    }

    /// Create a panel from an existing session (SSH or local).
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        let focus = cx.focus_handle();
        let tree = SpaceTree::new_terminal(view.clone(), focus);
        let active = tree.active();

        let _settings_sub = cx.observe(&TerminalSettings::global(cx), |_this, _settings, cx| {
            cx.notify();
        });
        view.read(cx).focus_handle(cx).focus(window, cx);

        let workspace_id = oneterm_state::AppState::primary_workspace_id(cx);
        let mut this = Self {
            workspace_id,
            tree,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),
            tab_title_override: None,
            _title_subs: Vec::new(),
            _settings_sub,
        };
        this.attach_split_ctx(&view, active, cx);
        this.rebuild_title_subs(cx);
        this
    }

    /// Helper to create an `Entity<Self>` from an existing session.
    pub fn from_session_entity(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::from_session(session, title, window, cx))
    }

    /// Helper to create an `Entity<Self>` (default local session).
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Create an entity bound to a specific dock/workspace.
    pub fn new_entity_in_workspace(
        workspace_id: EntityId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new_in_workspace(workspace_id, window, cx))
    }

    /// Helper to create an `Entity<Self>` with a specific shell kind.
    pub fn new_with_shell_entity(
        kind: ShellKind,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new_with_shell(kind, window, cx))
    }

    /// Spawn a fresh default local session + view.
    /// Returns `None` if the session could not be spawned (e.g. missing shell).
    /// The caller should handle the `None` case by showing an error state.
    pub(super) fn spawn_local_view(
        shell_kind_override: Option<ShellKind>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let (shell, scrollback_history) = {
            let settings = TerminalSettings::global(cx).read(cx);
            let shell = match shell_kind_override {
                Some(kind) => {
                    // Override the shell kind but keep other settings (utf8, cwd, env, args).
                    let mut s = settings.shell.clone();
                    s.kind = kind;
                    // Clear program so resolve_shell auto-detects for the new kind.
                    s.program = None;
                    s
                }
                None => settings.shell.clone(),
            };
            (shell, settings.scrollback_history)
        };
        let Some(factory) = oneterm_terminal::session_factory() else {
            log::error!("No session factory installed; cannot spawn local terminal.");
            return None;
        };
        let session: Box<dyn TerminalSession> =
            match factory.spawn_local(shell, PtySize { rows: 24, cols: 80 }, scrollback_history) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to spawn local terminal session: {e}");
                    return None;
                }
            };
        let session_entity = cx.new(|_| session);
        Some(cx.new(|cx| LocalTerminalView::new(session_entity, window, cx)))
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
    pub fn network_stats(&self, cx: &App) -> Option<oneterm_terminal::NetStats> {
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
    pub fn breadcrumb_label(&self, cx: &App) -> Option<String> {
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
    pub fn leaf_count(&self) -> usize {
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
    pub fn tab_label(&self, cx: &App) -> String {
        let mode = TerminalSettings::global(cx).read(cx).tab_title_mode;
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
    pub fn tab_label_with_title(&self, live_title: Option<&str>, cx: &App) -> String {
        let mode = TerminalSettings::global(cx).read(cx).tab_title_mode;
        let live = match mode {
            TabTitleMode::Osc => live_title,
            TabTitleMode::Default => None,
        };
        self.effective_tab_label(live)
    }

    /// The Agent Panel Space label for `space_id`: `single` for a one-Space tab,
    /// else `#N` (SpaceTree depth-first order — `docs/agent-panel-display.md` §5.1).
    pub fn space_label(&self, space_id: SpaceId) -> String {
        if self.tree.leaf_count() <= 1 {
            return "single".to_string();
        }
        match self.tree.leaf_index(space_id) {
            Some(i) => format!("#{}", i + 1),
            None => "single".to_string(),
        }
    }

    /// Whether `space_id` is the focused Space *and* this tab is the active tab.
    pub fn is_space_active(&self, space_id: SpaceId) -> bool {
        self.is_active && self.tree.active() == space_id
    }

    /// A weak handle to the containing `TabPanel` (for Agent Panel click-to-focus).
    pub fn tab_panel_weak(&self) -> Option<WeakEntity<TabPanel>> {
        self.tab_panel.clone()
    }

    /// Whether this tab has no terminal Spaces left (all empty).
    pub fn has_no_terminals(&self, _cx: &App) -> bool {
        self.tree.has_no_terminals()
    }

    /// Shut down all terminal sessions and cancel all tasks.
    /// Called by `Panel::on_removed`, last-space close, and error paths.
    /// Idempotent.
    pub fn shutdown(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
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
        self.tree
            .active_focus_handle(cx)
            .expect("active Space always has a focus handle")
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
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
        let mode = TerminalSettings::global(cx).read(cx).tab_title_mode;
        let session_title = self
            .tree
            .active_terminal()
            .and_then(|v| v.read(cx).session.read(cx).title());
        let live = match mode {
            TabTitleMode::Osc => session_title.as_deref(),
            TabTitleMode::Default => None,
        };
        let tab_label = self.tab_label_with_title(live, cx);
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
                div()
                    .id(title_label_id)
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .on_click({
                        let panel = cx.entity().clone();
                        let rename_title = rename_title.clone();
                        move |event, window, cx| {
                            if event.click_count() != 2 {
                                return;
                            }
                            cx.stop_propagation();
                            title::open_tab_title_dialog(
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

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }
        if active {
            self.publish_active_session(window, cx);
        }
    }
}
