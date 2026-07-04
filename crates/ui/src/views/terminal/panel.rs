//! [`TerminalPanel`] — leaf panel displaying one Terminal session.
//!
//! MVP: creates its own `LocalSession` (default cmd) + `LocalTerminalView`.
//! TODO: move session construction to the app layer to make SSH pluggable (the
//! View still uses `dyn TerminalSession`, only the factory changes).

use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
};
use oneterm_core::TerminalSession;
use oneterm_local::{LocalSession, PtySize};

use crate::state::{AppState, TerminalSettings};

use super::view::LocalTerminalView;

/// Panel displaying one Terminal session.
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    /// Reference to the `TabPanel` containing this panel — used for the close-tab button.
    tab_panel: Option<WeakEntity<TabPanel>>,
    /// Whether this panel is the currently selected tab in the `TabPanel`.
    ///
    /// We can't read the `TabPanel` inside `title()` (it is rendering at that
    /// point), so we mirror this state via the [`Panel::set_active`] hook, which
    /// the `TabPanel` calls whenever the active tab changes.
    is_active: bool,
    /// Tab title — "Terminal" for local, session label for SSH.
    tab_title: String,
}

impl TerminalPanel {
    /// Create a panel + spawn the default local session (cmd on Windows).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let view = cx.new(|cx| LocalTerminalView::new(session, window, cx));
        // Focus the terminal view right after creation — app startup + new tab.
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: "Terminal".to_string(),
        }
    }

    /// Create a panel from an existing session (SSH or local).
    ///
    /// The session is already spawned/connected, the panel just wraps the view.
    /// Used for SSH terminal tabs — `session` is a `Box<dyn TerminalSession>`
    /// from `SshSession::connect()`.
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),
        }
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

    /// Session network stats (SSH only — `None` for local).
    /// Used by the StatusBar to show network speed.
    pub fn network_stats(&self, cx: &App) -> Option<oneterm_core::NetStats> {
        self.view.read(cx).session.read(cx).network_stats()
    }

    /// Helper to create an `Entity<Self>` (default local session).
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to the terminal view — when the dock area focuses the panel,
        // the terminal view inside receives focus.
        self.view.read(cx).focus_handle(cx)
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_panel = self.tab_panel.clone();
        let panel_entity = cx.entity().clone();
        let theme = cx.theme().muted_foreground;
        // Active tab highlight color — taken from theme (`table.active.border`).
        let highlight = cx.theme().table_active_border;
        let is_active = self.is_active;

        h_flex()
            .id("tab-title")
            .relative()
            .h_full()
            .w_full()
            .min_w(px(100.))
            .items_center()
            .gap_1()
            // Active tab highlight — a 2px top border colored from the theme.
            // `Tab` wraps the title in an inner h_flex (30px tall, centered in the
            // 32px tab) + `overflow_hidden`, so this is the highest point reachable
            // from `title()` (the top edge of the inner box, ~1px below the tab edge).
            // Overflow left/right negatively to cover the full width; the excess is clipped.
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
            // Compensate for the Tab inner_h_flex's 12px right padding so the × sits against the right edge.
            .mr(-px(5.))
            // Middle-click on a tab → close that tab (even an inactive tab).
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
            // Tab title — flexible, truncated with ellipsis when narrow.
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(self.tab_title.clone()),
            )
            // Close button (×) — against the right edge of the tab.
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
                        // Prevent the click from propagating to the Tab (avoid activating the tab).
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
        // `TabPanel` calls this hook when the active tab changes → mirror it for `title()` to use.
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }

        // When this tab becomes active → extract SFTP from the session (if any)
        // and set it into AppState.active_sftp for SftpPanel to observe.
        // The next active tab will overwrite it — no need to set None on deactivate.
        if active {
            let (sftp, cwd_source, is_local) = {
                let session = self.view.read(cx).session.read(cx);
                (session.sftp(), session.cwd_source(), session.is_local())
            };
            AppState::global(cx).update(cx, |state, cx| {
                state.active_sftp = sftp;
                state.active_cwd_source = cwd_source;
                state.active_is_local = is_local;
                cx.notify();
            });

            // Auto-hide the Right Dock when this tab is a local shell.
            // The right dock hosts the Session/SFTP browser, which is only useful
            // for SSH sessions — hide it on local tabs to reclaim space.
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
                    crate::layout::workspace::set_right_dock_open(
                        &dock_area, !is_local, window, cx,
                    );
                }
            }
        }
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            .child(self.view.clone())
    }
}
