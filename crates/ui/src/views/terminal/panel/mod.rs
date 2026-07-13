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

use std::sync::Arc;

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
};
use oneterm_core::TerminalSession;
use oneterm_local::{LocalSession, PtySize};

use crate::state::{TabTitleMode, TerminalSettings};

use super::space::{DragTerminalTab, SpaceId, SpaceTree, SplitContext};
use super::view::{LocalTerminalView, TerminalViewEvent};

/// Panel displaying a Terminal Tab (a tree of Spaces).
pub struct TerminalPanel {
    /// The pane tree — leaves are terminals or empty placeholders.
    pub(super) tree: SpaceTree,
    /// Reference to the `TabPanel` containing this panel — used for the close-tab
    /// button and to remove the tab when the last Space closes.
    pub(super) tab_panel: Option<WeakEntity<TabPanel>>,
    /// Whether this panel is the currently selected tab in the `TabPanel`.
    pub(super) is_active: bool,
    /// Tab title — "Terminal" for local, session label for SSH.
    pub(super) tab_title: String,
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
    pub(super) fn spawn_local_view(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<LocalTerminalView> {
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
