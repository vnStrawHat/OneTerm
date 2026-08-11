//! "Quick Connect" dialog — enter connection and authentication details, then
//! connect immediately. Optionally save the session to the SSH session store.
//!
//! Unlike [`open_connect_dialog`](super::connect_dialog::open_connect_dialog)
//! (which requires an existing saved session), this dialog collects all
//! connection details in one place and connects directly on "Connect".

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use gpui::{App, AppContext, ClickEvent, ParentElement as _, Styled, Window, div, px};
use gpui_component::{
    WindowExt as _,
    button::Button,
    checkbox::Checkbox,
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_core::{ConnectionCancellation, HostKeyPolicy, SshConfig};
use oneterm_state::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::{ConnectButton, FieldRequirement, connect_ssh_session, field};
use crate::session_state::{SshAuthPreference, SshSession, SshSessionStore};

/// Open a dialog that collects SSH connection and authentication details.
/// Optionally save the non-secret session metadata to the SSH session store.
pub fn open_quick_connect_dialog(window: &mut Window, cx: &mut App) {
    // ── Input states ──────────────────────────────────────────────────
    let host_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("e.g. 192.168.1.10 or server.example.com")
    });
    let port_state = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
    let username_state =
        cx.new(|cx| InputState::new(window, cx).placeholder("root  or  root@host:port"));
    let auth_form = SshAuthForm::new(SshAuthPreference::Password, None, window, cx);

    // ── Save-to-store checkbox state ───────────────────────────────────
    let save_session = Rc::new(Cell::new(false));
    let connecting = Arc::new(AtomicBool::new(false));
    let connection_cancellation: Rc<RefCell<Option<ConnectionCancellation>>> =
        Rc::new(RefCell::new(None));

    // ── Shared connect logic ───────────────────────────────────────────
    let connect_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let host_state = host_state.clone();
        let port_state = port_state.clone();
        let username_state = username_state.clone();
        let auth_form = auth_form.clone();
        let save_session = save_session.clone();
        let connection_cancellation = connection_cancellation.clone();
        let connecting = connecting.clone();
        move |_, window, cx| {
            if connecting.load(Ordering::Relaxed) {
                return false;
            }
            let host_field = host_state.read(cx).value().trim().to_string();
            let port_field = port_state.read(cx).value().trim().to_string();
            let username_raw = username_state.read(cx).value().trim().to_string();

            // Parse "user@host:port" format from the username field.
            // If the username contains '@', split into user + host[:port].
            let (username, parsed_host, parsed_port) =
                if let Some((user, rest)) = username_raw.split_once('@') {
                    let (host, port) = if let Some((h, p)) = rest.rsplit_once(':') {
                        if let Ok(p) = p.parse::<u16>() {
                            (h.to_string(), Some(p))
                        } else {
                            (rest.to_string(), None)
                        }
                    } else {
                        (rest.to_string(), None)
                    };
                    (user.to_string(), Some(host), port)
                } else {
                    (username_raw, None, None)
                };

            // Use parsed host if the host field is empty.
            let host = if !host_field.is_empty() {
                host_field
            } else if let Some(h) = parsed_host {
                h
            } else {
                window.push_notification(
                    notify(NotificationType::Warning, "Host is required.", cx),
                    cx,
                );
                return false;
            };

            // Use parsed port if the port field is empty.
            let port: u16 = if !port_field.is_empty() {
                port_field.parse().unwrap_or(SshSession::DEFAULT_PORT)
            } else {
                parsed_port.unwrap_or(SshSession::DEFAULT_PORT)
            };

            if username.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Username is required.", cx),
                    cx,
                );
                return false;
            }

            let auth = match auth_form.take_auth(window, cx) {
                Ok(auth) => auth,
                Err(message) => {
                    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
                    return false;
                }
            };

            // Optionally save non-secret connection metadata to the SSH session store.
            if save_session.get() {
                let label = format!("{}@{}:{}", username, host, port);
                let session = SshSession {
                    label,
                    host: host.clone(),
                    port,
                    username: Some(username.clone()),
                    auth_method: auth_form.method(),
                    key_path: if auth_form.method() == SshAuthPreference::PrivateKey {
                        auth_form.key_path_value(cx)
                    } else {
                        None
                    },
                    color: None,
                    group: None,
                };
                SshSessionStore::global(cx).update(cx, |s, cx| {
                    s.add(session, cx);
                });
            }

            // Build the connection config and connect off-thread.
            let cfg = SshConfig {
                host,
                port,
                username,
                auth,
                cancellation: ConnectionCancellation::default(),
                host_key_policy: HostKeyPolicy::Strict,
                shell_integration: true,
            };
            let label = format!("{}@{}:{}", cfg.username, cfg.host, cfg.port);
            connecting.store(true, Ordering::Relaxed);
            window.refresh();
            let cancellation = connect_ssh_session(cfg, label, connecting.clone(), window, cx);
            *connection_cancellation.borrow_mut() = Some(cancellation);

            false
        }
    });

    let connect_button = cx.new(|_| ConnectButton::new(connect_logic.clone(), connecting.clone()));

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let connect_for_kb = connect_logic.clone();
        let cancellation_for_keyboard = connection_cancellation.clone();

        dialog
            .title("SSH Quick Connect")
            .w(px(440.))
            .content({
                let host_state = host_state.clone();
                let port_state = port_state.clone();
                let username_state = username_state.clone();
                let auth_form = auth_form.clone();
                let save_session = save_session.clone();
                move |content, _window, cx| {
                    content
                        .child(field(
                            "Host",
                            FieldRequirement::Required,
                            Input::new(&host_state),
                            cx,
                        ))
                        .child(field(
                            "Port",
                            FieldRequirement::Optional,
                            Input::new(&port_state),
                            cx,
                        ))
                        .child(field(
                            "Username",
                            FieldRequirement::Required,
                            Input::new(&username_state),
                            cx,
                        ))
                        .child(auth_form.render(cx))
                        .child(
                            div().pt_1().child(
                                Checkbox::new("save-session")
                                    .label("Save to SSH Sessions")
                                    .checked(save_session.get())
                                    .on_click({
                                        let save_session = save_session.clone();
                                        move |checked: &bool, _window, _cx| {
                                            save_session.set(*checked);
                                        }
                                    }),
                            ),
                        )
                }
            })
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
                    .child(connect_button.clone())
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
