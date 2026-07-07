//! [`TerminalPanel`] — a Terminal Tab hosting a tree of resizable **Spaces**.
//!
//! A panel used to wrap exactly one `LocalTerminalView`; it now owns a
//! [`SpaceTree`] whose leaves are terminals or empty placeholders. A tree with a
//! single leaf renders exactly like the old single-terminal panel. See
//! `docs/terminal-split/`.

use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
    resizable::ResizableState,
};
use oneterm_core::TerminalSession;
use oneterm_local::{LocalSession, PtySize};

use crate::state::{AppState, TabTitleMode, TerminalSettings};

use super::space::{
    CloseOutcome, DragTerminalTab, SpaceContent, SpaceId, SpaceLeaf, SpaceTree, SplitContext,
    SplitDir, render_node,
};
use super::view::{LocalTerminalView, TerminalViewEvent};

/// Panel displaying a Terminal Tab (a tree of Spaces).
pub struct TerminalPanel {
    /// The pane tree — leaves are terminals or empty placeholders.
    tree: SpaceTree,
    /// Reference to the `TabPanel` containing this panel — used for the close-tab
    /// button and to remove the tab when the last Space closes.
    tab_panel: Option<WeakEntity<TabPanel>>,
    /// Whether this panel is the currently selected tab in the `TabPanel`.
    is_active: bool,
    /// Tab title — "Terminal" for local, session label for SSH.
    tab_title: String,
    /// Subscriptions to every terminal leaf's `TitleChanged` — rebuilt whenever
    /// the set of terminals changes (split / close / drop / fill).
    _title_subs: Vec<Subscription>,
    /// Subscription to global `TerminalSettings` changes.
    _settings_sub: Subscription,
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
        let view = Self::spawn_local_view(window, cx);
        let focus = cx.focus_handle();
        let tree = SpaceTree::new_terminal(view.clone(), focus);
        let active = tree.active();

        let _settings_sub = cx.observe(&TerminalSettings::global(cx), |_this, _settings, cx| {
            cx.notify();
        });

        // Focus the terminal view right after creation — app startup + new tab.
        view.read(cx).focus_handle(cx).focus(window, cx);

        let mut this = Self {
            tree,
            tab_panel: None,
            is_active: false,
            tab_title: "Terminal".to_string(),
            _title_subs: Vec::new(),
            _settings_sub,
        };
        this.attach_split_ctx(&view, active, cx);
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

        let mut this = Self {
            tree,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),
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

    /// Spawn a fresh default local session + view.
    fn spawn_local_view(window: &mut Window, cx: &mut Context<Self>) -> Entity<LocalTerminalView> {
        let (shell, scrollback_history) = {
            let settings = TerminalSettings::global(cx).read(cx);
            (settings.shell.clone(), settings.scrollback_history)
        };
        let session: Entity<Box<dyn TerminalSession>> = cx.new(|_cx| {
            Box::new(
                LocalSession::spawn(shell, PtySize { rows: 24, cols: 80 }, scrollback_history)
                    .expect("spawn local session"),
            ) as Box<dyn TerminalSession>
        });
        cx.new(|cx| LocalTerminalView::new(session, window, cx))
    }

    /// Point `view`'s context menu at Space `space_id` in this panel.
    fn attach_split_ctx(
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
    fn rebuild_title_subs(&mut self, cx: &mut Context<Self>) {
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
    pub fn network_stats(&self, cx: &App) -> Option<oneterm_core::NetStats> {
        self.tree
            .active_terminal()?
            .read(cx)
            .session
            .read(cx)
            .network_stats()
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

    /// Whether this tab has no terminal Spaces left (all empty).
    pub fn has_no_terminals(&self, _cx: &App) -> bool {
        self.tree.has_no_terminals()
    }

    // ── Space operations ────────────────────────────────────────────

    /// Split Space `space_id` in `dir`; the new empty Space becomes active.
    pub fn split_active_at(
        &mut self,
        space_id: SpaceId,
        dir: SplitDir,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_id = self.tree.alloc_id();
        let empty = SpaceLeaf {
            id: new_id,
            content: SpaceContent::Empty,
            focus: cx.focus_handle(),
        };
        let state = cx.new(|_| ResizableState::default());
        self.tree.split(space_id, dir, empty, state);
        self.set_active_space(new_id, window, cx);
        cx.notify();
    }

    /// Close Space `space_id`. Closes the whole tab if it was the last Space.
    pub fn close_space(&mut self, space_id: SpaceId, window: &mut Window, cx: &mut Context<Self>) {
        let (outcome, removed) = self.tree.close(space_id);
        if let Some(view) = removed {
            view.read(cx).session.read(cx).close();
        }
        if outcome == CloseOutcome::LastSpaceClosed {
            if let Some(tp) = self.tab_panel.as_ref().and_then(|w| w.upgrade()) {
                let panel: Arc<dyn PanelView> = Arc::new(cx.entity());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
            return;
        }
        self.rebuild_title_subs(cx);
        let active = self.tree.active();
        self.set_active_space(active, window, cx);
        cx.notify();
    }

    /// Spawn a local shell directly into empty Space `space_id`.
    pub fn new_terminal_here(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = Self::spawn_local_view(window, cx);
        self.attach_split_ctx(&view, space_id, cx);
        self.tree.fill_empty(space_id, view);
        self.rebuild_title_subs(cx);
        self.set_active_space(space_id, window, cx);
        cx.notify();
    }

    /// Make Space `space_id` the active Space (focus it + refresh status bar).
    pub fn set_active_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tree.has_leaf(space_id) {
            return;
        }
        let changed = self.tree.active() != space_id;
        self.tree.set_active(space_id);
        if let Some(fh) = self.tree.active_focus_handle(cx) {
            fh.focus(window, cx);
        }
        if changed {
            if self.is_active {
                self.publish_active_session(window, cx);
            }
            cx.notify();
        }
    }

    /// Take the active Space's terminal view out of this tree, leaving it empty
    /// (and collapsing that Space if other Spaces remain). Used by drag-drop.
    pub fn take_active_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let id = self.tree.active();
        let view = self.tree.take_leaf_terminal(id)?;
        if self.tree.leaf_count() > 1 {
            let _ = self.tree.close(id);
            self.rebuild_title_subs(cx);
            let active = self.tree.active();
            self.set_active_space(active, window, cx);
            cx.notify();
        }
        Some(view)
    }

    /// Handle a Terminal Tab dropped onto empty Space `target`: move the source
    /// tab's active terminal into this Space (see `docs/terminal-split/03`).
    pub fn handle_tab_drop(
        &mut self,
        target: SpaceId,
        drag: &DragTerminalTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(src) = drag.panel.upgrade() else {
            return;
        };
        let is_self = src == cx.entity();

        let view = if is_self {
            // Dropping within the same tab: no-op for a single Space.
            if self.tree.leaf_count() == 1 {
                return;
            }
            self.take_active_terminal_view(window, cx)
        } else {
            src.update(cx, |sp, cx| sp.take_active_terminal_view(window, cx))
        };
        let Some(view) = view else {
            return;
        };

        self.attach_split_ctx(&view, target, cx);
        self.tree.fill_empty(target, view);
        self.rebuild_title_subs(cx);
        self.set_active_space(target, window, cx);

        // Remove the emptied source tab (only when the source is a different,
        // now-terminal-less panel).
        if !is_self && src.read(cx).has_no_terminals(cx) {
            if let Some(tp) = drag.tab_panel.upgrade() {
                let panel: Arc<dyn PanelView> = Arc::new(src.clone());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
        }
        cx.notify();
    }

    /// Publish the active Space's session into `AppState` (SFTP / cwd / locality)
    /// and apply the auto-hide-right-dock rule.
    fn publish_active_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sftp, cwd_source, is_local) = match self.tree.active_terminal() {
            Some(view) => {
                let s = view.read(cx).session.read(cx);
                (s.sftp(), s.cwd_source(), s.is_local())
            }
            None => (None, None, true),
        };
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            state.active_cwd_source = cwd_source;
            state.active_is_local = is_local;
            cx.notify();
        });

        let auto_hide = TerminalSettings::global(cx)
            .read(cx)
            .auto_hide_right_dock_on_local;
        if auto_hide {
            let dock_area = AppState::global(cx)
                .read(cx)
                .dock_area
                .as_ref()
                .and_then(|w| w.upgrade());
            if let Some(dock_area) = dock_area {
                crate::layout::workspace::set_right_dock_open(&dock_area, !is_local, window, cx);
            }
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
        let tab_label = resolve_tab_label(live, &self.tab_title);
        let drag_title: SharedString = tab_label.clone().into();

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
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
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

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tree.active();
        let single = self.tree.is_single();
        let panel = cx.entity().downgrade();
        let body = render_node(self.tree.root(), active, single, panel, window, cx);
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_title_is_used() {
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "user@host:24"),
            "user@host: ~/repo"
        );
        assert_eq!(resolve_tab_label(Some("cmd.exe"), "Terminal"), "cmd.exe");
    }

    #[test]
    fn none_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(None, "Terminal"), "Terminal");
        assert_eq!(resolve_tab_label(None, "prod-server"), "prod-server");
    }

    #[test]
    fn empty_title_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(Some(""), "Terminal"), "Terminal");
    }

    #[test]
    fn fallback_is_returned_by_value() {
        let label = resolve_tab_label(None, "Terminal");
        assert_eq!(label, "Terminal");
    }

    #[test]
    fn windows_drive_path_shortened_to_basename() {
        assert_eq!(
            resolve_tab_label(Some("C:\\Windows\\system32\\cmd.exe"), "Terminal"),
            "cmd.exe"
        );
        assert_eq!(
            resolve_tab_label(
                Some("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
                "Terminal"
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn posix_path_shortened_to_basename() {
        assert_eq!(resolve_tab_label(Some("/usr/bin/bash"), "Terminal"), "bash");
        assert_eq!(resolve_tab_label(Some("/bin/sh"), "Terminal"), "sh");
    }

    #[test]
    fn relative_or_descriptive_titles_not_trimmed() {
        assert_eq!(resolve_tab_label(Some("~/repo"), "Terminal"), "~/repo");
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "Terminal"),
            "user@host: ~/repo"
        );
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
    }

    #[test]
    fn trim_path_title_helper_directly() {
        assert_eq!(trim_path_title("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("/usr/bin/bash"), "bash");
        assert_eq!(trim_path_title("cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("user@host: ~/repo"), "user@host: ~/repo");
        assert_eq!(trim_path_title("  /usr/bin/zsh  "), "zsh");
    }
}
