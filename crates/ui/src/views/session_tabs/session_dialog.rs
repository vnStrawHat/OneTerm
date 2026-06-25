//! Dialog "New / Edit SSH Session" — tạo mới hoặc chỉnh sửa SSH session.
//!
//! Dialog có footer chứa 2 button: **Cancel** ([`DialogClose`]) và **Save**
//! ([`DialogAction`]). Khi Save → validate (Label & Host bắt buộc) →
//! `store.add` (tạo mới) hoặc `store.update` (chỉnh sửa) → auto-save
//! `ssh_session.json`.

use gpui::prelude::FluentBuilder as _;
use gpui::{App, AppContext, SharedString, Window, div, px};
use gpui::{IntoElement, ParentElement as _, Styled};
use gpui_component::{
    ActiveTheme, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogAction, DialogButtonProps, DialogClose, DialogFooter},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

use crate::state::{SshSession, SshSessionStore};

/// Mở dialog tạo mới (khi `edit` = `None`) hoặc chỉnh sửa (khi `edit` =
/// `Some((index, session))`) SSH session.
pub(crate) fn open_session_dialog(
    window: &mut Window,
    cx: &mut App,
    edit: Option<(usize, SshSession)>,
) {
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
pub(crate) fn field(
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
