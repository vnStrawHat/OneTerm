//! [`SessionPanel`] — leaf panel hiển thị danh sách SSH session.
//!
//! Render list label các SSH session load từ `ssh_session.json` (qua
//! [`crate::state::SshSessionStore`]) khi khởi động.
//!
//! - Left-click vào session item → mở dialog connect SSH.
//! - Right-click vào khu vực panel (trống) → context menu "New Session".
//! - Right-click vào 1 session item → context menu: Open, Delete, Property.
//! - "New Session" / "Property" → mở dialog (xem [`super::session_dialog`]).
//! - "Open" / left-click → mở dialog connect (xem [`super::connect_dialog`]).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, relative,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    menu::{ContextMenuExt, PopupMenuItem},
    v_flex,
};

use crate::actions::NewSession;
use crate::state::{SshSession, SshSessionStore};

use super::connect_dialog::open_connect_dialog;
use super::session_dialog::open_session_dialog;

/// Panel hiển thị danh sách SSH session.
///
/// `panel_name = "session"`.
pub struct SessionPanel {
    focus_handle: FocusHandle,
    store: Entity<SshSessionStore>,
}

impl SessionPanel {
    /// Tạo panel mới — bind vào global [`SshSessionStore`] và observe để
    /// re-render khi list session thay đổi.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            focus_handle: cx.focus_handle(),
            store,
        }
    }

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Action handler: mở dialog "New SSH Session" (tạo mới).
    pub(crate) fn on_new_session(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_session_dialog(window, cx, None);
    }
}

impl EventEmitter<PanelEvent> for SessionPanel {}

impl Focusable for SessionPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SessionPanel {
    fn panel_name(&self) -> &'static str {
        "session"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Session"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }
}

impl Render for SessionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let sessions = self.store.read(cx).sessions().to_vec();
        let focus = self.focus_handle.clone();

        // Header.
        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(format!("Sessions ({})", sessions.len())),
            )
            .child(
                Button::new("new-session-btn")
                    .small()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("New SSH Session")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_new_session(&NewSession, window, cx);
                    })),
            );

        // List rows.
        let mut list = v_flex().w_full().gap_0p5().p_1();
        for (ix, s) in sessions.iter().enumerate() {
            list = list.child(render_session_row(ix, s, &focus, cx));
        }

        // Empty state.
        let empty = h_flex()
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .text_sm()
            .child("No SSH session yet. Right-click → New Session.");

        div()
            .id("session-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_session))
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(header)
            .child(
                div()
                    .id("session-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(sessions.is_empty(), |t| t.child(empty))
                    .when(!sessions.is_empty(), |t| t.child(list))
                    // Right-click vào khu vực panel (không phải item) → New Session.
                    .context_menu({
                        let focus = focus.clone();
                        move |menu, _window, _cx| {
                            menu.action_context(focus.clone())
                                .menu("New Session", Box::new(NewSession))
                        }
                    }),
            )
    }
}

/// Render 1 row trong danh sách session: icon + label (trên) + host:port (dưới)
/// + left-click → connect + right-click → context menu Open / Delete / Property.
fn render_session_row(
    ix: usize,
    session: &SshSession,
    focus: &FocusHandle,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let title = SharedString::from(session.label.clone());
    let subtitle = SharedString::from(match &session.username {
        Some(u) => format!("{}@{}:{}", u, session.host, session.port),
        None => format!("{}:{}", session.host, session.port),
    });
    let focus = focus.clone();

    div()
        .id(("session-row", ix))
        .w_full()
        .px_2()
        .py_1p5()
        .rounded_md()
        .cursor_pointer()
        .hover(|t| t.bg(theme.muted))
        // Left-click → mở dialog connect SSH.
        .on_click(move |_, window, cx| {
            let session = SshSessionStore::global(cx)
                .read(cx)
                .sessions()
                .get(ix)
                .cloned();
            if let Some(s) = session {
                open_connect_dialog(s, ix, window, cx);
            }
        })
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_shrink_0()
                        .size_7()
                        .rounded_md()
                        .bg(theme.muted)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::SquareTerminal)
                                .xsmall()
                                .text_color(theme.foreground),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0()
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.foreground)
                                .line_height(relative(1.3))
                                .truncate()
                                .child(title),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .line_height(relative(1.3))
                                .truncate()
                                .child(subtitle),
                        ),
                ),
        )
        // Right-click vào item → context menu Open / Delete / Property.
        .context_menu(move |menu, _window, _cx| {
            menu.action_context(focus.clone())
                .item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
                    let session = SshSessionStore::global(cx)
                        .read(cx)
                        .sessions()
                        .get(ix)
                        .cloned();
                    if let Some(s) = session {
                        open_connect_dialog(s, ix, window, cx);
                    }
                }))
                .separator()
                .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                    SshSessionStore::global(cx).update(cx, |s, cx| {
                        s.remove(ix, cx);
                    });
                    window.push_notification("SSH session đã bị xoá.", cx);
                }))
                .separator()
                .item(
                    PopupMenuItem::new("Property").on_click(move |_, window, cx| {
                        let session = SshSessionStore::global(cx)
                            .read(cx)
                            .sessions()
                            .get(ix)
                            .cloned();
                        if let Some(s) = session {
                            open_session_dialog(window, cx, Some((ix, s)));
                        }
                    }),
                )
        })
}
