//! "Quick Connect" dialog — enter host, port, username, password and connect
//! immediately. Optionally save the session to the SSH session store.
//!
//! Unlike [`open_connect_dialog`](super::connect_dialog::open_connect_dialog)
//! (which requires an existing saved session), this dialog collects all
//! connection details in one place and connects directly on "Connect".

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext, ClickEvent, ParentElement as _, Styled, Window, div, px};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};
use oneterm_ui::dock::PanelView;

use oneterm_core::{SshAuthMethod, SshConfig};
use oneterm_terminal::PtySize;

use crate::session_state::{SshSession, SshSessionStore};
use oneterm_state::notif_ext::notify;
use oneterm_terminal_view::TerminalPanel;

use super::common::{add_ssh_terminal_to_dock, field};

/// Open a "Quick Connect" dialog — enter host, port, username, password and
/// connect immediately. Optionally save the session to the SSH session store.
pub fn open_quick_connect_dialog(window: &mut Window, cx: &mut App) {
    // ── Input states ──────────────────────────────────────────────────
    let host_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("e.g. 192.168.1.10 or server.example.com")
    });
    let port_state = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
    let username_state =
        cx.new(|cx| InputState::new(window, cx).placeholder("root  or  root@host:port"));
    let password_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("Enter password")
            .masked(true)
    });

    // ── Save-to-store checkbox state ───────────────────────────────────
    let save_session = Rc::new(Cell::new(false));

    // ── Shared connect logic ───────────────────────────────────────────
    let connect_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let host_state = host_state.clone();
        let port_state = port_state.clone();
        let username_state = username_state.clone();
        let password_state = password_state.clone();
        let save_session = save_session.clone();
        move |_, window, cx| {
            let host_field = host_state.read(cx).value().trim().to_string();
            let port_field = port_state.read(cx).value().trim().to_string();
            let username_raw = username_state.read(cx).value().trim().to_string();
            let password = password_state.read(cx).value().to_string();

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

            // Optionally save to the SSH session store.
            if save_session.get() {
                let label = format!("{}@{}:{}", username, host, port);
                let session = SshSession {
                    label,
                    host: host.clone(),
                    port,
                    username: Some(username.clone()),
                    color: None,
                    group: None,
                };
                SshSessionStore::global(cx).update(cx, |s, cx| {
                    s.add(session, cx);
                });
            }

            // Build SshConfig + connect asynchronously.
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
                shell_integration: true,
            };
            let label = format!("{}@{}:{}", cfg.username, cfg.host, cfg.port);

            let Some(factory) = oneterm_terminal::session_factory() else {
                window.push_notification(
                    notify(
                        NotificationType::Error,
                        "Internal error: no session factory installed.".to_string(),
                        cx,
                    ),
                    cx,
                );
                return true;
            };

            window
                .spawn(cx, async move |cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            factory.connect_ssh(cfg, PtySize { rows: 24, cols: 80 }, 10_000)
                        })
                        .await;

                    _ = cx.update(|window, cx| match result {
                        Ok(ssh_session) => {
                            let panel: Arc<dyn PanelView> = Arc::new(
                                TerminalPanel::from_session_entity(ssh_session, &label, window, cx),
                            );
                            add_ssh_terminal_to_dock(&panel, window, cx);
                            window.push_notification(
                                notify(
                                    NotificationType::Success,
                                    format!("Connected to \"{}\".", label),
                                    cx,
                                ),
                                cx,
                            );
                        }
                        Err(e) => {
                            window.push_notification(
                                notify(
                                    NotificationType::Error,
                                    format!("SSH connect failed: {e}"),
                                    cx,
                                ),
                                cx,
                            );
                        }
                    });
                })
                .detach();

            true
        }
    });

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let connect_for_click = connect_logic.clone();
        let connect_for_kb = connect_logic.clone();

        dialog
            .title("SSH Quick Connect")
            .w(px(440.))
            .content({
                let host_state = host_state.clone();
                let port_state = port_state.clone();
                let username_state = username_state.clone();
                let password_state = password_state.clone();
                let save_session = save_session.clone();
                move |content, _window, cx| {
                    content
                        .child(field("Host", true, Input::new(&host_state), cx))
                        .child(field("Port", false, Input::new(&port_state), cx))
                        .child(field("Username", true, Input::new(&username_state), cx))
                        .child(field(
                            "Password",
                            false,
                            Input::new(&password_state).mask_toggle(),
                            cx,
                        ))
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
