//! "Connect to SSH" dialog — enter credentials and connect over SSH.
//!
//! When the user clicks a session item (or selects "Open" in the context menu):
//! - If `SshSession.username = None` → the dialog asks for **username + password**.
//! - If `SshSession.username = Some` → the dialog asks for **password** only.
//!
//! Footer: **Cancel** + **Connect** — uses direct on_click to bypass
//! action dispatch through the focus chain (consistent with the SSH Session dialog).

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
    notification::NotificationType,
    v_flex,
};

use oneterm_ssh::{PtySize, SshAuthMethod, SshConfig, connect as ssh_connect};

use crate::notif_ext::notify;
use crate::state::{AppState, SshSession, SshSessionStore};

use crate::views::TerminalPanel;

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

/// Handler for the Connect button — validates inputs, builds SshConfig, connects.
fn on_connect_click(
    session: &SshSession,
    index: usize,
    username_state: &Option<gpui::Entity<InputState>>,
    password_state: &gpui::Entity<InputState>,
    save_username: bool,
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

    // 4. Build SshConfig + connect asynchronously (does not block the UI).
    //    Empty password → None auth (server does not require a password).
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
    let label = session.label.clone();

    window
        .spawn(cx, async move |cx| {
            // Run connect on the background executor — connect() uses block_on
            // internally, so it needs its own thread.
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

    true // close the dialog
}

/// Add the SSH terminal panel to the DockArea center.
fn add_ssh_terminal_to_dock(panel: &Arc<dyn PanelView>, window: &mut Window, cx: &mut App) {
    let dock_area = match AppState::global(cx).read(cx).dock_area.clone() {
        Some(d) => d,
        None => {
            log::error!("AppState.dock_area not initialized — cannot add SSH terminal tab");
            return;
        }
    };

    dock_area
        .update(cx, |dock, cx| {
            dock.add_panel(panel.clone(), DockPlacement::Center, None, window, cx);
        })
        .ok();
}

/// Parse the username field input — accepts 3 forms:
/// - `username`
/// - `username@host`
/// - `username@host:port`
///
/// Returns (username, host, port). Any part missing from the input keeps its default.
fn parse_user_host_port(raw: &str, default_host: &str, default_port: u16) -> (String, String, u16) {
    match raw.split_once('@') {
        Some((user, rest)) => {
            // rest = "host" or "host:port"
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

/// Server info banner (read-only).
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

/// Form field: label (with `*`) + input element.
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

/// Password field: label + masked input with mask_toggle + cleanable.
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


// ── Quick Connect dialog ────────────────────────────────────────────────

/// Open a "Quick Connect" dialog — enter host, port, username, password and
/// connect immediately. Optionally save the session to the SSH session store.
///
/// Unlike `open_connect_dialog` (which requires an existing saved session),
/// this dialog collects all connection details in one place and connects
/// directly on "Connect".
pub(crate) fn open_quick_connect_dialog(window: &mut Window, cx: &mut App) {
    // ── Input states ──────────────────────────────────────────────────
    let host_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("e.g. 192.168.1.10 or server.example.com")
    });
    let port_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("22")
    });
    let username_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("root  or  root@host:port")
    });
    let password_state = cx.new(|cx| {
        InputState::new(window, cx).placeholder("Enter password").masked(true)
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

            window
                .spawn(cx, async move |cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            ssh_connect(cfg, PtySize { rows: 24, cols: 80 }, 10_000)
                        })
                        .await;

                    _ =
                        cx.update(|window, cx| match result {
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
                        .child(field("Password", false, Input::new(&password_state).mask_toggle(), cx))
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
                    .child(
                        Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ),
                    )
                    .child(
                        Button::new("connect").label("Connect").primary().on_click(
                            move |_, window, cx| {
                                if connect_for_click(&ClickEvent::default(), window, cx) {
                                    window.close_dialog(cx);
                                }
                            },
                        ),
                    )
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| {
                        connect_for_kb(&ClickEvent::default(), window, cx)
                    }),
            )
    });
}
