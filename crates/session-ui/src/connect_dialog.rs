//! "Connect to SSH" dialog — enter credentials and connect over SSH.
//!
//! When the user clicks a session item (or selects "Open" in the context menu):
//! - If `SshSession.username = None`, the dialog also asks for a username.
//! - Authentication fields follow the saved password or private-key preference.
//!
//! Built on `oneterm_state::form_dialog::FormDialog`: **Cancel** + a stateful
//! **Connect** button ([`ConnectButton`]); Enter submits, Escape/Cancel abort a
//! connection attempt in flight.

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
    App, AppContext, Focusable as _, IntoElement as _, ParentElement as _, SharedString, Styled,
    Window, div,
};
use gpui_component::{
    WindowExt as _,
    checkbox::Checkbox,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_core::{ConnectionCancellation, HostKeyPolicy, SshConfig};
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_theme::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::{
    ConnectButton, SshConnectRequest, connect_ssh_session, defer_initial_focus_once,
    parse_user_host_port, server_info_banner,
};
use crate::session_state::{SshSession, SshSessionId, SshSessionStore};

/// Open the SSH connect dialog.
///
/// - `session`: SSH session info from the store.
/// - `id`: the session's stable id (used to save a username the user enters).
///
/// When the saved session has no username, the dialog asks for one in addition to
/// rendering the session's preferred authentication fields.
pub(crate) fn open_connect_dialog(
    session: SshSession,
    id: SshSessionId,
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

    // ── Shared connect logic (Connect button + keyboard Enter) ──
    let connect_logic: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new({
        let username_state = username_state.clone();
        let auth_form = auth_form.clone();
        let session = session.clone();
        let save_username = save_username.clone();
        let connection_cancellation = connection_cancellation.clone();
        let connecting = connecting.clone();
        move |window, cx| {
            if connecting.load(Ordering::Relaxed) {
                return false;
            }
            on_connect_click(
                &session,
                id,
                &username_state,
                &auth_form,
                save_username.get(),
                &connection_cancellation,
                connecting.clone(),
                window,
                cx,
            )
        }
    });

    let connect_button = cx.new(|_| ConnectButton::new(connect_logic.clone(), connecting));
    let initial_focus_pending = Rc::new(Cell::new(true));
    let cancel_connection = {
        let connection_cancellation = connection_cancellation.clone();
        move |_: &mut Window, _: &mut App| {
            if let Some(cancellation) = connection_cancellation.borrow().as_ref() {
                cancellation.cancel();
            }
        }
    };
    let initial_focus = {
        let username_state = username_state.clone();
        let auth_form = auth_form.clone();
        move |window: &mut Window, cx: &mut App| {
            let focus = match username_state.as_ref() {
                Some(state) => state.read(cx).focus_handle(cx),
                None => auth_form.focus_handle(cx),
            };
            defer_initial_focus_once(&initial_focus_pending, focus, window, cx);
        }
    };

    FormDialog::new(
        title,
        move |content, _window, cx| {
            content
                // Server info banner (read-only).
                .child(server_info_banner(
                    SharedString::from(server_info.clone()),
                    cx,
                ))
                // Username field (only when ask_username).
                .when_some(username_state.as_ref(), |content, st| {
                    content.child(labelled_field(
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
        },
        move |window, cx| connect_logic(window, cx),
    )
    .confirm_element(move |_, _| connect_button.clone().into_any_element())
    .on_cancel(cancel_connection)
    .on_render(initial_focus)
    .open(window, cx);
}

/// Handler for the Connect button — validates inputs, builds SshConfig, connects.
#[allow(clippy::too_many_arguments)]
fn on_connect_click(
    session: &SshSession,
    id: SshSessionId,
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
            match parse_user_host_port(&raw) {
                Ok(parsed) => (
                    parsed.user,
                    parsed.host.unwrap_or_else(|| session.host.clone()),
                    parsed.port.unwrap_or(session.port),
                ),
                Err(error) => {
                    window.push_notification(
                        notify(NotificationType::Warning, error.to_string(), cx),
                        cx,
                    );
                    return false;
                }
            }
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
            s.update(id, updated, cx);
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
    let cancellation = connect_ssh_session(
        cfg,
        SshConnectRequest::new(session.label.clone()),
        connecting,
        window,
        cx,
    );
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
