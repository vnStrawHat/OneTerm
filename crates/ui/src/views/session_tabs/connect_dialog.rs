//! Dialog "Connect to SSH" — nhập credentials + kết nối SSH.
//!
//! Khi user click vào session item (hoặc chọn "Open" trong context menu):
//! - Nếu `SshSession.username = None` → dialog hỏi **username + password**.
//! - Nếu `SshSession.username = Some` → dialog chỉ hỏi **password**.
//!
//! Footer: **Cancel** + **Connect** — dùng direct on_click để bypass
//! action dispatch qua focus chain (thống nhất với SSH Session dialog).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, ClickEvent, Focusable as _, IntoElement, ParentElement as _, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogButtonProps, DialogFooter},
    dock::{DockPlacement, PanelView},
    h_flex,
    input::{Input, InputState},
    v_flex,
};

use myterm2_ssh::{PtySize, SshAuthMethod, SshConfig, connect as ssh_connect};

use crate::state::{AppState, SshSession, SshSessionStore};
use crate::views::TerminalPanel;

/// Mở dialog connect SSH.
///
/// - `session`: thông tin SSH session từ store.
/// - `index`: vị trí trong store (để update username nếu user nhập mới).
///
/// Logic phân nhánh:
/// - `session.username = None` → dialog hỏi username + password.
/// - `session.username = Some` → dialog chỉ hỏi password.
pub(crate) fn open_connect_dialog(
    session: SshSession,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let ask_username = session.username.is_none();

    // Dialog title.
    let title: String = if ask_username {
        format!("Connect to {}", session.label)
    } else {
        let u = session.username.as_deref().unwrap_or("");
        format!(
            "Connect to {} ({}@{}:{})",
            session.label, u, session.host, session.port
        )
    };

    // Server info banner text.
    let server_info = match &session.username {
        Some(u) => format!("ssh://{}@{}:{}", u, session.host, session.port),
        None => format!("ssh://{}:{}", session.host, session.port),
    };

    // Password state — luôn cần, masked.
    let password_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Enter password")
            .masked(true)
    });

    // Username state — chỉ tạo khi cần hỏi.
    let username_state: Option<gpui::Entity<InputState>> = if ask_username {
        Some(cx.new(|cx| InputState::new(window, cx).placeholder("e.g. root, ubuntu, admin — hoặc root@host:port")))
    } else {
        None
    };


    // Save username flag — default KHÔNG lưu (user phải tick checkbox).
    let save_username = Rc::new(Cell::new(false));
    // Clone cho save_logic closure.
    let password_ok = password_state.clone();
    let username_ok = username_state.clone();
    let session_ok = session.clone();
    let title_ok = title.clone();

    // ── Shared connect logic (dùng cho cả button on_click và keyboard on_ok) ──
    let connect_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let username_ok = username_ok.clone();
        let password_ok = password_ok.clone();
        let session_ok = session_ok.clone();
        let save_username = save_username.clone();
        move |_, window, cx| {
            let save = save_username.get();
            on_connect_click(
                &session_ok,
                index,
                &username_ok,
                &password_ok,
                save,
                window,
                cx,
            )
        }
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        // Focus username (nếu chưa có) hoặc password (nếu đã có username).
        let focus_handle = if ask_username {
            username_state.as_ref().unwrap().read(cx).focus_handle(cx)
        } else {
            password_state.read(cx).focus_handle(cx)
        };
        focus_handle.focus(window, cx);
        // Clone connect_logic cho button on_click và keyboard on_ok
        let connect_for_click = connect_logic.clone();
        let connect_for_kb = connect_logic.clone();
        dialog
            .title(title_ok.clone())
            .w(px(440.))
            .content({
                let server_info = server_info.clone();
                let username_state = username_state.clone();
                let password_state = password_state.clone();
                let save_username = save_username.clone();
                move |content, _window, cx| {
                    content
                        // Server info banner (read-only).
                        .child(server_info_banner(
                            SharedString::from(server_info.clone()),
                            cx,
                        ))
                        // Username field (chỉ khi ask_username).
                        .when_some(username_state.as_ref(), |content, st| {
                            content.child(field("Username", true, Input::new(st), cx))
                        })
                        // Password field (luôn).
                        .child(password_field(&password_state, cx))
                        // Checkbox "Lưu username" — chỉ hiện khi ask_username.
                        .when_some(username_state.as_ref(), |content, _st| {
                            content.child(
                                div()
                                    .pt_1()
                                    .child(
                                        Checkbox::new("save-username")
                                            .label("Lưu username vào session")
                                            .checked(save_username.get())
                                            .on_click({
                                                let save_username = save_username.clone();
                                                move |checked: &bool, _window, _cx| {
                                                    save_username.set(*checked);
                                                }
                                            }),
                                    ),
                            )
                        })
                }
            })
            // Footer: Cancel + Connect — dùng direct on_click thay vì DialogAction/DialogClose
            // để bypass action dispatch qua focus chain.
            .footer({
                DialogFooter::new()
                    .child(Button::new("cancel").label("Cancel").outline().on_click(
                        |_, window, cx| {
                            window.close_dialog(cx);
                        },
                    ))
                    .child(Button::new("connect").label("Connect").primary().on_click(
                        move |_, window, cx| {
                            if connect_for_click(&ClickEvent::default(), window, cx) {
                                window.close_dialog(cx);
                            }
                        },
                    ))
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| connect_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}

/// Handler cho nút Connect — validate inputs, tạo SshConfig, kết nối.
fn on_connect_click(
    session: &SshSession,
    index: usize,
    username_state: &Option<gpui::Entity<InputState>>,
    password_state: &gpui::Entity<InputState>,
    save_username: bool,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    // 1. Đọc + parse username field.
    //    Chấp nhận: "username", "username@host", "username@host:port".
    //    User input cái nào thì overwrite cái đó.
    let (username, host, port) = match username_state {
        Some(st) => {
            let raw = st.read(cx).value().trim().to_string();
            if raw.is_empty() {
                window.push_notification("Username là bắt buộc.", cx);
                return false;
            }
            parse_user_host_port(&raw, &session.host, session.port)
        }
        None => (
            session.username.clone().unwrap_or_default(),
            session.host.clone(),
            session.port,
        ),
    };

    // 2. Đọc password (không bắt buộc — có thể để trống).
    let password = password_state.read(cx).value().to_string();

    // 3. (Tuỳ chọn) Lưu username/host/port vào store — chỉ khi user tick checkbox.
    if save_username && username_state.is_some() {
        let mut updated = session.clone();
        updated.username = Some(username.clone());
        updated.host = host.clone();
        updated.port = port;
        SshSessionStore::global(cx).update(cx, |s, cx| {
            s.update(index, updated, cx);
        });
    }

    // 4. Tạo SshConfig + kết nối async (không block UI).
    //    Password rỗng → None auth (server không yêu cầu password).
    let auth = if password.is_empty() {
        SshAuthMethod::None
    } else {
        SshAuthMethod::Password { password }
    };
    let cfg = SshConfig {
        host,
        port,
        username,
        auth,
    };
    let label = session.label.clone();

    window
        .spawn(cx, async move |cx| {
            // Chạy connect trên background executor — connect() dùng block_on
            // bên trong nên cần thread riêng.
            let result = cx
                .background_executor()
                .spawn(async move { ssh_connect(cfg, PtySize { rows: 24, cols: 80 }, 10_000) })
                .await;

            _ =
                cx.update(|window, cx| match result {
                    Ok(ssh_session) => {
                        let panel: Arc<dyn PanelView> = Arc::new(
                            TerminalPanel::from_session_entity(ssh_session, &label, window, cx),
                        );
                        add_ssh_terminal_to_dock(&panel, window, cx);
                        window.push_notification(format!("Connected to \"{}\".", label), cx);
                    }
                    Err(e) => {
                        window.push_notification(format!("SSH connect failed: {e}"), cx);
                    }
                });
        })
        .detach();

    true // đóng dialog
}

/// Thêm SSH terminal panel vào DockArea center.
fn add_ssh_terminal_to_dock(panel: &Arc<dyn PanelView>, window: &mut Window, cx: &mut App) {
    let dock_area = match AppState::global(cx).read(cx).dock_area.clone() {
        Some(d) => d,
        None => {
            tracing::error!("AppState.dock_area chưa khởi tạo — không thể add SSH terminal tab");
            return;
        }
    };

    dock_area
        .update(cx, |dock, cx| {
            dock.add_panel(panel.clone(), DockPlacement::Center, None, window, cx);
        })
        .ok();
}

/// Parse input username field — chấp nhận 3 dạng:
/// - `username`
/// - `username@host`
/// - `username@host:port`
///
/// Trả về (username, host, port). Phần nào không có trong input thì giữ default.
fn parse_user_host_port(raw: &str, default_host: &str, default_port: u16) -> (String, String, u16) {
    match raw.split_once('@') {
        Some((user, rest)) => {
            // rest = "host" hoặc "host:port"
            match rest.rsplit_once(':') {
                Some((h, p_str)) => {
                    let port = p_str.parse::<u16>().unwrap_or(default_port);
                    (user.to_string(), h.to_string(), port)
                }
                None => (user.to_string(), rest.to_string(), default_port),
            }
        }
        None => (raw.to_string(), default_host.to_string(), default_port),
    }
}

// ── UI helpers ───────────────────────────────────────────────────────

/// Banner hiển thị thông tin server (read-only).
fn server_info_banner(info: SharedString, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .w_full()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(theme.muted)
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(info)
}

/// Field form: label (có dấu `*`) + input element.
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

/// Password field: label + masked input với mask_toggle + cleanable.
fn password_field(state: &gpui::Entity<InputState>, _cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(SharedString::from("Password")),
        )
        .child(Input::new(state).mask_toggle().cleanable(true))
}
