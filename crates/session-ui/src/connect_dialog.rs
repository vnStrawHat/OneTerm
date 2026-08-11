//! "Connect to SSH" dialog — enter credentials and connect over SSH.
//!
//! When the user clicks a session item (or selects "Open" in the context menu):
//! - If `SshSession.username = None`, the dialog also asks for a username.
//! - Authentication fields follow the saved password or private-key preference.
//!
//! Footer: **Cancel** + **Connect** — uses direct on_click to bypass
//! action dispatch through the focus chain (consistent with the SSH Session dialog).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
use gpui::FocusHandle;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, ClickEvent, Focusable as _, ParentElement as _, SharedString, Styled, Window,
    div, px,
};
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
use super::common::{
    ConnectButton, FieldRequirement, connect_ssh_session, defer_initial_focus_once, field,
    parse_user_host_port, server_info_banner,
};
use crate::session_state::{SshSession, SshSessionStore};

/// Open the SSH connect dialog.
///
/// - `session`: SSH session info from the store.
/// - `index`: position in the store (used to update the username if the user enters a new one).
///
/// When the saved session has no username, the dialog asks for one in addition to
/// rendering the session's preferred authentication fields.
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

    let auth_form = SshAuthForm::new(session.auth_method, session.key_path.as_deref(), window, cx);

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
    let connecting = Arc::new(AtomicBool::new(false));
    let connection_cancellation: Rc<RefCell<Option<ConnectionCancellation>>> =
        Rc::new(RefCell::new(None));
    let auth_for_connect = auth_form.clone();
    let username_for_connect = username_state.clone();
    let session_for_connect = session.clone();
    let title_ok = title.clone();

    // ── Shared connect logic (used by both the button on_click and keyboard on_ok) ──
    let connect_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let username_for_connect = username_for_connect.clone();
        let auth_for_connect = auth_for_connect.clone();
        let session_for_connect = session_for_connect.clone();
        let save_username = save_username.clone();
        let connection_cancellation = connection_cancellation.clone();
        let connecting = connecting.clone();
        move |_, window, cx| {
            if connecting.load(Ordering::Relaxed) {
                return false;
            }
            let save = save_username.get();
            on_connect_click(
                &session_for_connect,
                index,
                &username_for_connect,
                &auth_for_connect,
                save,
                &connection_cancellation,
                connecting.clone(),
                window,
                cx,
            )
        }
    });

    let connect_button = cx.new(|_| ConnectButton::new(connect_logic.clone(), connecting.clone()));
    let initial_focus_pending = Rc::new(Cell::new(true));

    window.open_dialog(cx, move |dialog, window, cx| {
        let initial_focus = match username_state.as_ref() {
            Some(state) => state.read(cx).focus_handle(cx),
            None => auth_form.focus_handle(cx),
        };
        defer_initial_focus_once(&initial_focus_pending, initial_focus, window, cx);

        // Clone connect_logic for keyboard on_ok; the footer owns the Connect button.
        let connect_for_kb = connect_logic.clone();
        let cancellation_for_keyboard = connection_cancellation.clone();
        dialog
            .title(title_ok.clone())
            .w(px(440.))
            .content({
                let server_info = server_info.clone();
                let username_state = username_state.clone();
                let auth_form = auth_form.clone();
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
                            content.child(field(
                                "Username",
                                FieldRequirement::Required,
                                Input::new(st),
                                cx,
                            ))
                        })
                        .child(auth_form.render(cx))
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

/// Handler for the Connect button — validates inputs, builds SshConfig, connects.
#[allow(clippy::too_many_arguments)]
fn on_connect_click(
    session: &SshSession,
    index: usize,
    username_state: &Option<gpui::Entity<InputState>>,
    auth_form: &SshAuthForm,
    save_username: bool,
    connection_cancellation: &Rc<RefCell<Option<ConnectionCancellation>>>,
    connecting: Arc<AtomicBool>,
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

    // 2. Validate and collect the selected authentication material.
    let auth = match auth_form.take_auth(window, cx) {
        Ok(auth) => auth,
        Err(message) => {
            window.push_notification(notify(NotificationType::Warning, message, cx), cx);
            return false;
        }
    };

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

    // 4. Build the connection config and connect off-thread.
    let cfg = SshConfig {
        host,
        port,
        username,
        auth,
        cancellation: ConnectionCancellation::default(),
        host_key_policy: HostKeyPolicy::Strict,
        shell_integration: true,
    };
    connecting.store(true, Ordering::Relaxed);
    window.refresh();
    let cancellation =
        connect_ssh_session(cfg, session.label.clone(), None, connecting, window, cx);
    *connection_cancellation.borrow_mut() = Some(cancellation);

    false // keep the dialog open until the background attempt completes
}

#[cfg(test)]
mod tests {
    use gpui::{
        Context, InteractiveElement as _, IntoElement, Render, TestAppContext, VisualTestContext,
    };

    use super::*;

    struct FocusTestView {
        initial: FocusHandle,
        user_selected: FocusHandle,
    }

    impl Render for FocusTestView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .track_focus(&self.initial)
                .child(div().track_focus(&self.user_selected))
        }
    }

    #[gpui::test]
    fn initial_dialog_focus_is_not_reapplied_after_focus_moves(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_window, cx| FocusTestView {
            initial: cx.focus_handle(),
            user_selected: cx.focus_handle(),
        });
        let cx: &mut VisualTestContext = cx;
        let initial_focus_pending = Cell::new(true);
        let initial = view.read_with(cx, |view, _| view.initial.clone());
        let user_selected = view.read_with(cx, |view, _| view.user_selected.clone());

        cx.update(|window, cx| {
            defer_initial_focus_once(&initial_focus_pending, initial.clone(), window, cx);
        });
        cx.run_until_parked();
        assert!(cx.update(|window, _cx| initial.is_focused(window)));

        cx.update(|window, cx| user_selected.focus(window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            defer_initial_focus_once(&initial_focus_pending, initial, window, cx);
        });
        cx.run_until_parked();

        assert!(cx.update(|window, _cx| user_selected.is_focused(window)));
    }
}
