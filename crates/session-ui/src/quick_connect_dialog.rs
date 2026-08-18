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

use gpui::{
    App, AppContext, IntoElement, ParentElement as _, Styled, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    WindowExt as _,
    checkbox::Checkbox,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_core::{
    ConnectionCancellation, HostKeyPolicy, SshConfig, SshDuplicateAuth, SshDuplicateConfig,
};
use oneterm_state::commands::SshDuplicateCompletion;
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_state::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::{
    ConnectButton, SshConnectRequest, connect_ssh_session, defer_initial_focus_once, parse_port,
    parse_user_host_port,
};
use crate::session_state::{SshAuthPreference, SshSession, SshSessionStore};

#[derive(Clone, Copy)]
enum QuickConnectKind {
    New,
    Duplicate,
}

enum QuickConnectMode {
    New,
    Duplicate {
        config: SshDuplicateConfig,
        initial_cwd: Option<std::path::PathBuf>,
        completion: SshDuplicateCompletion,
    },
}

impl QuickConnectMode {
    fn kind(&self) -> QuickConnectKind {
        match self {
            Self::New => QuickConnectKind::New,
            Self::Duplicate { .. } => QuickConnectKind::Duplicate,
        }
    }
}

fn save_session_option(
    kind: QuickConnectKind,
    save_session: Rc<Cell<bool>>,
) -> Option<impl IntoElement> {
    match kind {
        QuickConnectKind::Duplicate => None,
        QuickConnectKind::New => Some(
            div().pt_1().child(
                Checkbox::new("save-session")
                    .label("Save to SSH Sessions")
                    .checked(save_session.get())
                    .on_click(move |checked: &bool, _window, _cx| {
                        save_session.set(*checked);
                    }),
            ),
        ),
    }
}

/// Open a dialog that collects SSH connection and authentication details.
/// Optionally save the non-secret session metadata to the SSH session store.
pub fn open_quick_connect_dialog(window: &mut Window, cx: &mut App) {
    open_quick_connect_dialog_internal(QuickConnectMode::New, window, cx);
}

/// Open the SSH Duplicate Session dialog with non-secret fields prefilled.
pub fn open_duplicate_ssh_dialog(
    config: SshDuplicateConfig,
    cwd: Option<std::path::PathBuf>,
    completion: SshDuplicateCompletion,
    window: &mut Window,
    cx: &mut App,
) {
    open_quick_connect_dialog_internal(
        QuickConnectMode::Duplicate {
            config,
            initial_cwd: cwd,
            completion,
        },
        window,
        cx,
    );
}

fn open_quick_connect_dialog_internal(mode: QuickConnectMode, window: &mut Window, cx: &mut App) {
    let kind = mode.kind();
    let is_duplicate = matches!(kind, QuickConnectKind::Duplicate);
    let (prefill, initial_cwd, completion) = match mode {
        QuickConnectMode::New => (None, None, None),
        QuickConnectMode::Duplicate {
            config,
            initial_cwd,
            completion,
        } => (Some(config), initial_cwd, Some(completion)),
    };
    let (host, port, username, auth_method, key_path, shell_integration) = match prefill {
        Some(config) => {
            let (method, key_path) = match config.auth {
                SshDuplicateAuth::PrivateKey { key_path } => {
                    (SshAuthPreference::PrivateKey, Some(key_path))
                }
                SshDuplicateAuth::None | SshDuplicateAuth::Password => {
                    (SshAuthPreference::Password, None)
                }
            };
            (
                config.host,
                config.port.to_string(),
                config.username,
                method,
                key_path,
                config.shell_integration,
            )
        }
        None => (
            String::new(),
            String::new(),
            String::new(),
            SshAuthPreference::Password,
            None,
            true,
        ),
    };

    // ── Input states ──────────────────────────────────────────────────
    let host_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("e.g. 192.168.1.10 or server.example.com")
            .default_value(host)
    });
    let port_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("22")
            .default_value(port)
    });
    let username_state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("root  or  root@host:port")
            .default_value(username)
    });
    let auth_form = SshAuthForm::new(auth_method, key_path.as_deref(), window, cx);

    // ── Save-to-store checkbox state ───────────────────────────────────
    let save_session = Rc::new(Cell::new(false));
    let connecting = Arc::new(AtomicBool::new(false));
    let connection_cancellation: Rc<RefCell<Option<ConnectionCancellation>>> =
        Rc::new(RefCell::new(None));

    // ── Shared connect logic (Connect button + keyboard Enter) ──
    let connect_logic: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new({
        let host_state = host_state.clone();
        let port_state = port_state.clone();
        let username_state = username_state.clone();
        let auth_form = auth_form.clone();
        let save_session = save_session.clone();
        let connection_cancellation = connection_cancellation.clone();
        let connecting = connecting.clone();
        let initial_cwd = initial_cwd.clone();
        let completion = completion.clone();
        move |window, cx| {
            if connecting.load(Ordering::Relaxed) {
                return false;
            }
            let host_field = host_state.read(cx).value().trim().to_string();
            let port_field = port_state.read(cx).value().trim().to_string();
            let username_raw = username_state.read(cx).value().trim().to_string();

            let target = match resolve_target(&username_raw, &host_field, &port_field) {
                Ok(target) => target,
                Err(message) => {
                    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
                    return false;
                }
            };

            let auth = match auth_form.take_auth(window, cx) {
                Ok(auth) => auth,
                Err(message) => {
                    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
                    return false;
                }
            };

            // Optionally save non-secret connection metadata to the SSH session
            // store — only once the connection succeeded (CORR-54), so a typo
            // never becomes a saved session.
            let on_connected: Option<Rc<dyn Fn(&mut App)>> = save_session.get().then(|| {
                let session = SshSession {
                    label: target.label(),
                    host: target.host.clone(),
                    port: target.port,
                    username: Some(target.username.clone()),
                    auth_method: auth_form.method(),
                    key_path: if auth_form.method() == SshAuthPreference::PrivateKey {
                        auth_form.key_path_value(cx)
                    } else {
                        None
                    },
                    color: None,
                    group: None,
                };
                Rc::new(move |cx: &mut App| {
                    SshSessionStore::global(cx).update(cx, |s, cx| {
                        s.add(session.clone(), cx);
                    });
                }) as Rc<dyn Fn(&mut App)>
            });

            // Build the connection config and connect off-thread.
            let request = SshConnectRequest {
                label: target.label(),
                initial_cwd: initial_cwd.clone(),
                completion: completion.clone(),
                on_connected,
            };
            let cfg = SshConfig {
                host: target.host,
                port: target.port,
                username: target.username,
                auth,
                cancellation: ConnectionCancellation::default(),
                host_key_policy: HostKeyPolicy::Strict,
                shell_integration,
            };
            connecting.store(true, Ordering::Relaxed);
            window.refresh();
            let cancellation = connect_ssh_session(cfg, request, connecting.clone(), window, cx);
            *connection_cancellation.borrow_mut() = Some(cancellation);

            false
        }
    });

    let connect_button = cx.new(|_| ConnectButton::new(connect_logic.clone(), connecting));
    let initial_focus_pending = Rc::new(Cell::new(is_duplicate));
    let cancel_connection = {
        let connection_cancellation = connection_cancellation.clone();
        move |_: &mut Window, _: &mut App| {
            if let Some(cancellation) = connection_cancellation.borrow().as_ref() {
                cancellation.cancel();
            }
        }
    };
    let initial_focus = {
        let auth_form = auth_form.clone();
        move |window: &mut Window, cx: &mut App| {
            if is_duplicate {
                defer_initial_focus_once(
                    &initial_focus_pending,
                    auth_form.secret_focus_handle(cx),
                    window,
                    cx,
                );
            }
        }
    };

    FormDialog::new(
        if is_duplicate {
            "Duplicate SSH Session"
        } else {
            "SSH Quick Connect"
        },
        move |content, _window, cx| {
            content
                .child(labelled_field(
                    "Host",
                    FieldRequirement::Required,
                    Input::new(&host_state),
                    cx,
                ))
                .child(labelled_field(
                    "Port",
                    FieldRequirement::Optional,
                    Input::new(&port_state),
                    cx,
                ))
                .child(labelled_field(
                    "Username",
                    FieldRequirement::Required,
                    Input::new(&username_state),
                    cx,
                ))
                .child(auth_form.render(cx))
                .when_some(
                    save_session_option(kind, save_session.clone()),
                    |content, option| content.child(option),
                )
        },
        move |window, cx| connect_logic(window, cx),
    )
    .confirm_element(move |_, _| connect_button.clone().into_any_element())
    .on_cancel(cancel_connection)
    .on_render(initial_focus)
    .open(window, cx);
}

/// The connection target resolved from the three quick-connect fields.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectTarget {
    username: String,
    host: String,
    port: u16,
}

impl ConnectTarget {
    fn label(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }
}

/// Combine the Username (`user[@host[:port]]`), Host and Port fields.
///
/// Precedence: an explicit Host / Port field wins over the host / port parsed
/// from the username; an empty Port falls back to the parsed port, then to 22.
/// A malformed username or port is reported with a corrective message.
fn resolve_target(
    username_raw: &str,
    host_field: &str,
    port_field: &str,
) -> Result<ConnectTarget, String> {
    let parsed = parse_user_host_port(username_raw).map_err(|error| error.to_string())?;
    let host = if !host_field.is_empty() {
        host_field.to_string()
    } else {
        parsed.host.ok_or_else(|| "Host is required.".to_string())?
    };
    let port = if !port_field.is_empty() {
        parse_port(port_field).map_err(|error| error.to_string())?
    } else {
        parsed.port.unwrap_or(SshSession::DEFAULT_PORT)
    };
    if parsed.user.is_empty() {
        return Err("Username is required.".to_string());
    }
    Ok(ConnectTarget {
        username: parsed.user,
        host,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(username: &str, host: &str, port: u16) -> ConnectTarget {
        ConnectTarget {
            username: username.into(),
            host: host.into(),
            port,
        }
    }

    /// TEST-19: explicit Host / Port fields win over the parts parsed from the
    /// username; missing parts fall back to the parsed ones, then to 22.
    #[test]
    fn explicit_fields_take_precedence_over_the_username_string() {
        assert_eq!(
            resolve_target("root@parsed.test:2200", "typed.test", "22"),
            Ok(target("root", "typed.test", 22))
        );
        assert_eq!(
            resolve_target("root@parsed.test:2200", "", ""),
            Ok(target("root", "parsed.test", 2200))
        );
        assert_eq!(
            resolve_target("root@parsed.test", "", ""),
            Ok(target("root", "parsed.test", 22))
        );
        assert_eq!(
            resolve_target("root", "typed.test", ""),
            Ok(target("root", "typed.test", 22))
        );
    }

    #[test]
    fn missing_or_invalid_parts_are_reported() {
        assert_eq!(
            resolve_target("root", "", ""),
            Err("Host is required.".to_string())
        );
        assert_eq!(
            resolve_target("", "typed.test", ""),
            Err("Username is required.".to_string())
        );
        assert!(
            resolve_target("root@host:abc", "", "")
                .unwrap_err()
                .contains("65535")
        );
        assert!(
            resolve_target("root", "typed.test", "99999")
                .unwrap_err()
                .contains("65535")
        );
    }

    #[test]
    fn save_option_element_is_built_only_for_quick_connect() {
        assert!(
            save_session_option(QuickConnectKind::Duplicate, Rc::new(Cell::new(false))).is_none()
        );
        assert!(save_session_option(QuickConnectKind::New, Rc::new(Cell::new(false))).is_some());
    }
}
