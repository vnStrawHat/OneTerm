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
    App, AppContext, ClickEvent, Focusable as _, ParentElement as _, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    dialog::{DialogButtonProps, DialogFooter},
    dock::PanelView,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_ssh::{PtySize, SshAuthMethod, SshConfig, connect as ssh_connect};

use crate::notif_ext::notify;
use crate::state::{SshSession, SshSessionStore};
use crate::views::TerminalPanel;

use super::common::{add_ssh_terminal_to_dock, field, parse_user_host_port, password_field,
    server_info_banner};

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