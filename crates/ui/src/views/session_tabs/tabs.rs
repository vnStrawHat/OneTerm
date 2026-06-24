//! [`SessionPanel`] — leaf panel hiển thị danh sách SSH session.
//!
//! Render list label các SSH session load từ `ssh_session.json` (qua
//! [`crate::state::SshSessionStore`]) khi khởi động.
//!
//! - Right-click vào khu vực panel (trống) → context menu "New Session".
//! - Right-click vào 1 session item → context menu: Open, Delete, Property.
//! - "New Session" / "Property" → mở Dialog (Save + Cancel ở footer) để tạo
//!   mới hoặc chỉnh sửa SSH session → lưu vào `ssh_session.json`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window, div, px, relative,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogAction, DialogButtonProps, DialogClose, DialogFooter},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    input::{Input, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    v_flex,
};

use crate::actions::NewSession;
use crate::state::{SshSession, SshSessionStore};

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

/// Mở dialog tạo mới (khi `edit` = `None`) hoặc chỉnh sửa (khi `edit` =
/// `Some((index, session))`) SSH session.
///
/// Dialog có footer chứa 2 button: **Cancel** ([`DialogClose`]) và **Save**
/// ([`DialogAction`]). Khi Save → validate (Label & Host bắt buộc) →
/// `store.add` (tạo mới) hoặc `store.update` (chỉnh sửa) → auto-save
/// `ssh_session.json`.
fn open_session_dialog(window: &mut Window, cx: &mut App, edit: Option<(usize, SshSession)>) {
    let is_edit = edit.is_some();
    let edit_index = edit.as_ref().map(|(ix, _)| *ix);
    let title: &'static str = if is_edit {
        "Edit SSH Session"
    } else {
        "New SSH Session"
    };

    // Giá trị prefill (rỗng nếu tạo mới).
    let (label_val, host_val, port_val, user_val) = match &edit {
        Some((_, s)) => (
            s.label.clone(),
            s.host.clone(),
            s.port.to_string(),
            s.username.clone().unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new(), String::new()),
    };

    // Tạo InputState (prefill nếu edit) — persist qua các lần re-render dialog.
    let label_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("e.g. Production Server");
        if !label_val.is_empty() {
            st.set_value(label_val, window, cx);
        }
        st
    });
    let host_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("e.g. 192.168.1.10");
        if !host_val.is_empty() {
            st.set_value(host_val, window, cx);
        }
        st
    });
    let port_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("22");
        if !port_val.is_empty() {
            st.set_value(port_val, window, cx);
        }
        st
    });
    let user_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("optional");
        if !user_val.is_empty() {
            st.set_value(user_val, window, cx);
        }
        st
    });

    // Clone cho on_ok closure (đọc value khi Save).
    let label_ok = label_state.clone();
    let host_ok = host_state.clone();
    let port_ok = port_state.clone();
    let user_ok = user_state.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title(title)
            .w(px(440.))
            .content({
                // Clone entity trước khi move vào inner `Fn` closure
                // (outer closure là `Fn`, không thể move captured value ra).
                let label_state = label_state.clone();
                let host_state = host_state.clone();
                let port_state = port_state.clone();
                let user_state = user_state.clone();
                move |content, _window, cx| {
                    content
                        .child(field("Label", true, Input::new(&label_state), cx))
                        .child(field("Host", true, Input::new(&host_state), cx))
                        .child(field("Port", false, Input::new(&port_state), cx))
                        .child(field("Username", false, Input::new(&user_state), cx))
                }
            })
            // Footer: Cancel (đóng dialog) + Save (dispatch ConfirmDialog → on_ok).
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(Button::new("cancel").label("Cancel").outline()),
                    )
                    .child(DialogAction::new().child(Button::new("save").label("Save").primary())),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok({
                        // Clone entity trước khi move vào on_ok `Fn` closure.
                        let label_ok = label_ok.clone();
                        let host_ok = host_ok.clone();
                        let port_ok = port_ok.clone();
                        let user_ok = user_ok.clone();
                        move |_, window, cx| {
                            let label = label_ok.read(cx).value().trim().to_string();
                            let host = host_ok.read(cx).value().trim().to_string();
                            if label.is_empty() || host.is_empty() {
                                window.push_notification("Label và Host là bắt buộc.", cx);
                                return false;
                            }
                            let port: u16 = port_ok
                                .read(cx)
                                .value()
                                .trim()
                                .parse()
                                .unwrap_or(SshSession::DEFAULT_PORT);
                            let username = {
                                let u = user_ok.read(cx).value().trim().to_string();
                                if u.is_empty() { None } else { Some(u) }
                            };
                            let session = SshSession {
                                label,
                                host,
                                port,
                                username,
                            };
                            let store = SshSessionStore::global(cx);
                            match edit_index {
                                Some(ix) => store.update(cx, |s, cx| s.update(ix, session, cx)),
                                None => store.update(cx, |s, cx| s.add(session, cx)),
                            }
                            window.push_notification(
                                if is_edit {
                                    "SSH session đã được cập nhật."
                                } else {
                                    "SSH session đã được lưu."
                                },
                                cx,
                            );
                            true
                        }
                    }),
            )
    });
}

/// Render 1 field form: label (có dấu `*` nếu bắt buộc) + input element.
fn field(
    label: &'static str,
    required: bool,
    input: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let danger = cx.theme().danger;
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(SharedString::from(label))
                .when(required, |t| t.child(div().text_color(danger).child("*"))),
        )
        .child(input)
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
/// + context menu (right-click) với Open / Delete / Property.
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
                    let label = SshSessionStore::global(cx)
                        .read(cx)
                        .sessions()
                        .get(ix)
                        .map(|s| s.label.clone())
                        .unwrap_or_default();
                    window.push_notification(format!("Open \"{label}\": chưa triển khai."), cx);
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
