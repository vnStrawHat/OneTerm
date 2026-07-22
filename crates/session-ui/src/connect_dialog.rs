//! "Connect to SSH" dialog — enter credentials and connect over SSH.
//!
//! When the user clicks a session item (or selects "Open" in the context menu):
//! - If `SshSession.username = None` → the dialog asks for **username + password**.
//! - If `SshSession.username = Some` → the dialog asks for **password** only.
//!
//! Footer: **Cancel** + **Connect** — uses direct on_click to bypass
//! action dispatch through the focus chain (consistent with the SSH Session dialog).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, ClickEvent, Focusable as _, ParentElement as _, SharedString, Styled, Window,
    div, px,
};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_core::{ConnectionCancellation, HostKeyPolicy, SecretString, SshAuthMethod, SshConfig};

use crate::session_state::{SshSession, SshSessionStore};
use oneterm_state::notif_ext::notify;

use super::common::{
    connect_ssh_session, field, parse_user_host_port, password_field, server_info_banner,
};

/// Open the SSH connect dialog.
///
/// - `session`: SSH session info from the store.
/// - `index`: position in the store (used to update the username if the user enters a new one).
///
/// Branching logic:
/// - `session.username = None` → dialog asks for username + password.
/// - `session.username = Some` → dialog asks for password only.
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

    // Password state — always needed, masked.
    let password_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Enter password")
            .masked(true)
    });

    // Username state — only created when needed.
    let username_state: Option<gpui::Entity<InputState>> = if ask_username {
        Some(cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. root, ubuntu, admin — or root@host:port")
        }))
    } else {
        None
    };

    // Save-username flag — defaults to NOT saving (the user must tick the checkbox).
    let save_username = Rc::new(Cell::new(false));
    let connection_cancellation: Rc<RefCell<Option<ConnectionCancellation>>> =
        Rc::new(RefCell::new(None));
    // Clone for the save_logic closure.
    let password_ok = password_state.clone();
    let username_ok = username_state.clone();
    let session_ok = session.clone();
    let title_ok = title.clone();

    // ── Shared connect logic (used by both the button on_click and keyboard on_ok) ──
    let connect_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let username_ok = username_ok.clone();
        let password_ok = password_ok.clone();
        let session_ok = session_ok.clone();
        let save_username = save_username.clone();
        let connection_cancellation = connection_cancellation.clone();
        move |_, window, cx| {
            let save = save_username.get();
            on_connect_click(
                &session_ok,
                index,
                &username_ok,
                &password_ok,
                save,
                &connection_cancellation,
                window,
                cx,
            )
        }
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        // Focus username (if not yet set) or password (if username already exists).
        let focus_handle = if ask_username {
            username_state.as_ref().unwrap().read(cx).focus_handle(cx)
        } else {
            password_state.read(cx).focus_handle(cx)
        };
        focus_handle.focus(window, cx);
        // Clone connect_logic for the button on_click and keyboard on_ok.
        let connect_for_click = connect_logic.clone();
        let connect_for_kb = connect_logic.clone();
        let cancellation_for_keyboard = connection_cancellation.clone();
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
                        // Username field (only when ask_username).
                        .when_some(username_state.as_ref(), |content, st| {
                            content.child(field("Username", true, Input::new(st), cx))
                        })
                        // Password field (always).
                        .child(password_field(&password_state, cx))
                        // "Save username" checkbox — only shown when ask_username.
                        .when_some(username_state.as_ref(), |content, _st| {
                            content.child(
                                div().pt_1().child(
                                    Checkbox::new("save-username")
                                        .label("Save username to session")
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
            // Footer: Cancel + Connect — uses direct on_click instead of DialogAction/DialogClose
            // to bypass action dispatch through the focus chain.
            .footer({
                let connection_cancellation = connection_cancellation.clone();
                DialogFooter::new()
                    .child(Button::new("cancel").label("Cancel").outline().on_click(
                        move |_, window, cx| {
                            if let Some(cancellation) = connection_cancellation.borrow().as_ref() {
                                cancellation.cancel();
                            }
                            window.close_dialog(cx);
                        },
                    ))
                    .child(Button::new("connect").label("Connect").primary().on_click(
                        move |_, window, cx| {
                            let _ = connect_for_click(&ClickEvent::default(), window, cx);
                        },
                    ))
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(move |_, _, _| {
                        if let Some(cancellation) = cancellation_for_keyboard.borrow().as_ref() {
                            cancellation.cancel();
                        }
                        true
                    })
                    .on_ok(move |_, window, cx| connect_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}

/// Handler for the Connect button — validates inputs, builds SshConfig, connects.
#[allow(clippy::too_many_arguments)]
fn on_connect_click(
    session: &SshSession,
    index: usize,
    username_state: &Option<gpui::Entity<InputState>>,
    password_state: &gpui::Entity<InputState>,
    save_username: bool,
    connection_cancellation: &Rc<RefCell<Option<ConnectionCancellation>>>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    // 1. Read + parse the username field.
    //    Accepts: "username", "username@host", "username@host:port".
    //    Whatever the user types overwrites the corresponding value.
    let (username, host, port) = match username_state {
        Some(st) => {
            let raw = st.read(cx).value().trim().to_string();
            if raw.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Username is required.", cx),
                    cx,
                );
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

    // 2. Read password (optional — may be left empty).
    let password = password_state.read(cx).value().to_string();

    // 3. (Optional) Save username/host/port to the store — only when the user ticks the checkbox.
    if save_username && username_state.is_some() {
        let mut updated = session.clone();
        updated.username = Some(username.clone());
        updated.host = host.clone();
        updated.port = port;
        SshSessionStore::global(cx).update(cx, |s, cx| {
            s.update(index, updated, cx);
        });
    }

    // 4. Build a zeroizing config, clear the UI field, and connect off-thread.
    let auth = if password.is_empty() {
        SshAuthMethod::None
    } else {
        SshAuthMethod::Password {
            password: SecretString::new(password),
        }
    };
    let cfg = SshConfig {
        host,
        port,
        username,
        auth,
        cancellation: ConnectionCancellation::default(),
        host_key_policy: HostKeyPolicy::Strict,
        shell_integration: true,
    };
    password_state.update(cx, |state, cx| state.set_value("", window, cx));
    let cancellation = connect_ssh_session(cfg, session.label.clone(), window, cx);
    *connection_cancellation.borrow_mut() = Some(cancellation);

    false // keep the dialog open until the background attempt completes
}
