//! "Connect to SSH" dialog — enter credentials and connect over SSH.
//!
//! When the user clicks a session item (or selects "Open" in the context menu):
//! - If `SshSession.username = None`, the dialog also asks for a username.
//! - Authentication fields follow the saved password or private-key preference.
//! - A saved password (`US-0158`) shows as "Using saved password" for the
//!   target and every jump hop once the vault is unlocked (one unlock covers
//!   the chain); Enter connects. A typed target password can be saved; it is
//!   sealed only after the server accepted it.
//!
//! Built on `oneterm_state::form_dialog::FormDialog`: **Cancel** + a stateful
//! **Connect** button ([`ConnectButton`]); Enter submits, Escape/Cancel abort a
//! connection attempt in flight.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
use gpui::FocusHandle;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Focusable as _, Global, IntoElement as _, ParentElement as _, SharedString,
    Styled, Window, div,
};
use gpui_component::{
    Disableable as _, WindowExt as _,
    checkbox::Checkbox,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_core::{
    AppError, ConnectPhase, ConnectionCancellation, HostKeyPolicy, SecretString, SshAuthMethod,
    SshConfig,
};
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, control_label, labelled_field};
use oneterm_state::vault_dialogs::ensure_unlocked;
use oneterm_state::{VaultPhase, VaultState};
use oneterm_theme::notif_ext::notify;
use oneterm_vault::{SealContext, SealedSecret, SecretPurpose};

use super::auth_form::{SavedPassword, SshAuthForm};
use super::common::{
    ConnectButton, InlineError, SshConnectRequest, connect_ssh_session, defer_initial_focus_once,
    parse_user_host_port, server_info_banner,
};
use super::jump_hops::{HopSpec, JumpHopForms};
use crate::session_state::{SshAuthPreference, SshSession, SshSessionId, SshSessionStore};

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

    // The jump chain is resolved now, so a broken reference is reported before
    // any credential is typed.
    let hop_specs: Result<Vec<HopSpec>, String> = SshSessionStore::global(cx)
        .read(cx)
        .jump_chain(session.jump_host, Some(id))
        .map_err(|error| error.to_string())
        .and_then(|chain| chain.iter().map(HopSpec::from_entry).collect());
    let hop_forms = match hop_specs {
        Ok(specs) => JumpHopForms::new(specs, window, cx),
        Err(message) => {
            window.push_notification(notify(NotificationType::Warning, message, cx), cx);
            return;
        }
    };
    let auth_form = SshAuthForm::new(session.auth_method, session.key_path.as_deref(), window, cx);
    auth_form.set_saved(SavedPassword::of(&session));
    auth_form.offer_save(session.auth_method == SshAuthPreference::Password);
    let mut password_forms: Vec<SshAuthForm> = hop_forms.forms().cloned().collect();
    if was_rejected(id, &session, cx) {
        // Rejected after a host-key approval closed the last dialog: open with
        // the input and "Update saved password" ticked (rework D2).
        auth_form.reveal_input(REJECTED_NOTE, true);
    } else {
        password_forms.push(auth_form.clone());
    }
    let unlock_first = activate_saved_passwords(&password_forms, cx);

    // Username state — only created when needed.
    let username_state: Option<gpui::Entity<InputState>> = if ask_username {
        Some(cx.new(|cx| {
            InputState::new(window, cx).placeholder("e.g. root, ubuntu, admin — or root@host:port")
        }))
    } else {
        None
    };

    // Failure shown in the dialog, beside the toast (`US-0118`).
    let inline_error = {
        let mut inputs = auth_form.secret_inputs();
        // A corrected jump-host password must clear the error too.
        inputs.extend(hop_forms.secret_inputs());
        inputs.extend(username_state.clone());
        InlineError::new(&inputs, cx)
    };

    // Save-username flag — defaults to NOT saving (the user must tick the checkbox).
    let save_username = Rc::new(Cell::new(false));
    let connecting = Arc::new(AtomicBool::new(false));
    let connection_cancellation: Rc<RefCell<Option<ConnectionCancellation>>> =
        Rc::new(RefCell::new(None));

    // ── Shared connect logic (Connect button + keyboard Enter) ──
    let attempt: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new({
        let username_state = username_state.clone();
        let auth_form = auth_form.clone();
        let hop_forms = hop_forms.clone();
        let session = session.clone();
        let save_username = save_username.clone();
        let connection_cancellation = connection_cancellation.clone();
        let connecting = connecting.clone();
        let inline_error = inline_error.clone();
        move |window, cx| {
            if connecting.load(Ordering::Relaxed) {
                return false;
            }
            // A retry starts clean; the failure of this attempt replaces it.
            inline_error.clear();
            on_connect_click(
                &session,
                id,
                &username_state,
                &auth_form,
                &hop_forms,
                save_username.get() || auth_form.save_ticked(),
                &connection_cancellation,
                connecting.clone(),
                inline_error.clone(),
                window,
                cx,
            )
        }
    });

    // Saving a typed password needs the vault unlocked **before** the attempt,
    // never after a connection is up (`ssh-password-save-flow.md` § 5).
    let connect_logic: Rc<dyn Fn(&mut Window, &mut App) -> bool> = Rc::new({
        let auth_form = auth_form.clone();
        let connecting = connecting.clone();
        move |window, cx| {
            if connecting.load(Ordering::Relaxed) || !auth_form.save_ticked() {
                return attempt(window, cx);
            }
            if auth_form.typed_is_empty(cx) {
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Enter the password to save, or untick Save password.",
                        cx,
                    ),
                    cx,
                );
                return false;
            }
            let state = VaultState::try_global(cx);
            let phase = state.as_ref().map(|state| state.read(cx).phase());
            match phase {
                Some(VaultPhase::Unlocked) => attempt(window, cx),
                Some(VaultPhase::Locked | VaultPhase::Uninitialised) => {
                    let attempt = attempt.clone();
                    ensure_unlocked(window, cx, move |window, cx| {
                        attempt(window, cx);
                    });
                    false
                }
                Some(VaultPhase::Unavailable) | None => {
                    let reason = state
                        .and_then(|state| state.read(cx).unavailable_reason().map(str::to_owned))
                        .unwrap_or_else(|| "no master password store.".to_owned());
                    window.push_notification(
                        notify(
                            NotificationType::Warning,
                            format!(
                                "Saved passwords are unavailable: {reason} Untick Save password to connect."
                            ),
                            cx,
                        ),
                        cx,
                    );
                    false
                }
            }
        }
    });

    let connect_button = cx.new(|_| ConnectButton::new(connect_logic.clone(), connecting));
    let initial_focus_pending = Rc::new(Cell::new(true));
    let refocus = initial_focus_pending.clone();
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
        let hop_forms = hop_forms.clone();
        move |window: &mut Window, cx: &mut App| {
            // The first field still to type; when every password is saved,
            // the saved row, so Enter connects (intake Q4).
            let focus = match username_state.as_ref() {
                Some(state) => Some(state.read(cx).focus_handle(cx)),
                None => hop_forms
                    .first_secret_focus(cx)
                    .or_else(|| auth_form.focus_handle(cx))
                    .or_else(|| hop_forms.forms().find_map(SshAuthForm::saved_focus_handle)),
            };
            if let Some(focus) = focus {
                defer_initial_focus_once(&initial_focus_pending, focus, window, cx);
            }
        }
    };

    FormDialog::new(
        title,
        move |content, _window, cx| {
            content
                .child(hop_forms.render(cx))
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
                .child(auth_form.render(true, cx))
                // "Save username" checkbox — only shown when ask_username. A
                // saved password is bound to a username, so ticking "Save
                // password" ticks and locks this one too.
                .when_some(username_state.as_ref(), |content, _st| {
                    let forced = auth_form.save_ticked();
                    content.child(
                        div().pt_1().child(
                            Checkbox::new("save-username")
                                .items_center()
                                .accessibility_label("Save username to session")
                                .child(control_label("Save username to session"))
                                .checked(save_username.get() || forced)
                                .disabled(forced)
                                .on_click({
                                    let save_username = save_username.clone();
                                    move |checked: &bool, _window, _cx| {
                                        save_username.set(*checked);
                                    }
                                }),
                        ),
                    )
                })
                // The failure of the last attempt, under the fields it is about.
                .when_some(inline_error.render(None, cx), |content, error| {
                    content.child(error)
                })
        },
        move |window, cx| connect_logic(window, cx),
    )
    .confirm_element(move |_, _| connect_button.clone().into_any_element())
    .on_cancel(cancel_connection)
    .on_render(initial_focus)
    .open(window, cx);

    if unlock_first {
        // Stacked on the connect dialog; Cancel ("type the passwords instead")
        // leaves the ordinary inputs.
        ensure_unlocked(window, cx, move |window, _| {
            for form in &password_forms {
                form.use_saved(true);
            }
            refocus.set(true);
            window.refresh();
        });
    }
}

/// Put every form with a saved password into saved mode when the vault is
/// unlocked, or say why it cannot be used. Returns whether the vault must be
/// unlocked first (it is locked).
fn activate_saved_passwords(forms: &[SshAuthForm], cx: &App) -> bool {
    let saved: Vec<&SshAuthForm> = forms.iter().filter(|form| form.has_saved()).collect();
    if saved.is_empty() {
        return false;
    }
    let state = VaultState::try_global(cx);
    let state = state.as_ref().map(|state| state.read(cx));
    let note = match state.map(VaultState::phase) {
        Some(VaultPhase::Unlocked) => {
            for form in &saved {
                form.use_saved(true);
            }
            return false;
        }
        Some(VaultPhase::Locked) => return true,
        Some(VaultPhase::Uninitialised) => {
            "Saved password unavailable: the master password was reset. Type the password."
                .to_owned()
        }
        Some(VaultPhase::Unavailable) | None => format!(
            "Saved password unavailable: {}",
            state
                .and_then(VaultState::unavailable_reason)
                .unwrap_or("no master password store.")
        ),
    };
    for form in saved {
        form.set_note(note.clone());
    }
    false
}

const REJECTED_NOTE: &str = "The server rejected the saved password. Type the current one.";

/// Which saved password a failure rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rejected {
    Target,
    Hop,
}

/// Sessions whose saved password the server rejected, with the envelope it
/// rejected (in memory only). The next connect dialog opens that row as an
/// input with "Update saved password" ticked, as long as the session still
/// holds that envelope; a successful connect forgets it (rework D2).
#[derive(Default)]
struct RejectedSavedPasswords(HashMap<SshSessionId, SealedSecret>);

impl Global for RejectedSavedPasswords {}

fn was_rejected(id: SshSessionId, session: &SshSession, cx: &App) -> bool {
    let rejected = cx
        .try_global::<RejectedSavedPasswords>()
        .and_then(|rejected| rejected.0.get(&id));
    rejected.is_some() && rejected == session.saved_password()
}

/// A rejected saved password (`ssh-password-save-flow.md` § 6): the row it was
/// used in turns into an empty input. The saved value is **not** removed (a
/// lockout or a typo on the server must not destroy it); a new one replaces it
/// only when the target connects with "Update saved password" ticked.
fn reveal_rejected_saved_password(
    error: &AppError,
    target: &SshAuthForm,
    hops: &JumpHopForms,
) -> Option<Rejected> {
    let AppError::Connect {
        phase: ConnectPhase::Authentication,
        message,
    } = error
    else {
        return None;
    };
    match hops.form_for_error(message) {
        Some(hop) if hop.uses_saved() => {
            hop.reveal_input(
                "The server rejected the saved password. Type the current one; update it in that jump host's session.",
                false,
            );
            Some(Rejected::Hop)
        }
        Some(_) => None,
        None if target.uses_saved() => {
            target.reveal_input(REJECTED_NOTE, true);
            Some(Rejected::Target)
        }
        None => None,
    }
}

/// After a rejected saved password: remember it for the next dialog and,
/// when the connect dialog is already gone (a host-key approval closed it),
/// say what to do in the toast instead of the row.
fn remember_rejection(
    rejected: Rejected,
    id: SshSessionId,
    session: &SshSession,
    window: &mut Window,
    cx: &mut App,
) {
    if let (Rejected::Target, Some(sealed)) = (rejected, session.saved_password()) {
        cx.default_global::<RejectedSavedPasswords>()
            .0
            .insert(id, sealed.clone());
    }
    if window.has_active_dialog(cx) {
        return;
    }
    let message = match rejected {
        Rejected::Target => format!(
            "The saved password for \"{}\" was rejected. Open the session to type the current one.",
            session.label
        ),
        Rejected::Hop => format!(
            "A jump host's saved password for \"{}\" was rejected. Update it in that jump host's session.",
            session.label
        ),
    };
    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
}

/// Seal the password the server just accepted and store it on session `id`
/// (`ssh-password-save-flow.md` § 5): never before the connection is up, so a
/// wrong password is never saved.
fn save_connected_password(
    id: SshSessionId,
    password: &SecretString,
    context: &SealContext<'_>,
    window: &mut Window,
    cx: &mut App,
) {
    let sealed = match VaultState::try_global(cx) {
        Some(state) => state
            .update(cx, |state, cx| {
                state.seal(SecretPurpose::SshPassword, context, password, cx)
            })
            .map_err(|error| error.message()),
        None => Err("saved passwords are unavailable.".to_owned()),
    };
    let stored = sealed.and_then(|sealed| {
        SshSessionStore::global(cx).update(cx, |store, cx| {
            store.set_sealed_password(id, context, sealed, cx)
        })
    });
    let (kind, message) = match stored {
        Ok(()) => (NotificationType::Success, "Password saved.".to_owned()),
        Err(reason) => (
            NotificationType::Warning,
            format!("Connected, but the password was not saved: {reason}"),
        ),
    };
    window.push_notification(notify(kind, message, cx), cx);
}

/// Handler for the Connect button — validates inputs, builds SshConfig, connects.
#[allow(clippy::too_many_arguments)]
fn on_connect_click(
    session: &SshSession,
    id: SshSessionId,
    username_state: &Option<gpui::Entity<InputState>>,
    auth_form: &SshAuthForm,
    hop_forms: &JumpHopForms,
    save_username: bool,
    connection_cancellation: &Rc<RefCell<Option<ConnectionCancellation>>>,
    connecting: Arc<AtomicBool>,
    inline_error: InlineError,
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

    // 2. Validate and collect the selected authentication material, hops first.
    let jump_hops = match hop_forms.take_hops(cx) {
        Ok(hops) => hops,
        Err(message) => {
            window.push_notification(notify(NotificationType::Warning, message, cx), cx);
            return false;
        }
    };
    let auth = match auth_form.take_auth(cx) {
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

    // 4. Build the connection config and connect off-thread. A password to
    //    save is kept (zeroizing) until the server has accepted it.
    let to_save = match (auth_form.save_ticked(), &auth) {
        (true, SshAuthMethod::Password { password }) => Some(password.clone()),
        _ => None,
    };
    let on_connected = {
        let (host, username) = (host.clone(), username.clone());
        Rc::new(move |window: &mut Window, cx: &mut App| {
            if cx.has_global::<RejectedSavedPasswords>() {
                cx.global_mut::<RejectedSavedPasswords>().0.remove(&id);
            }
            if let Some(password) = &to_save {
                let context = SealContext {
                    host: &host,
                    port,
                    username: &username,
                };
                save_connected_password(id, password, &context, window, cx);
            }
        }) as Rc<dyn Fn(&mut Window, &mut App)>
    };
    let cfg = SshConfig {
        host,
        port,
        username,
        auth,
        cancellation: ConnectionCancellation::default(),
        host_key_policy: HostKeyPolicy::Strict,
        shell_integration: true,
        jump_hops,
        port_forwards: session.port_forwards.clone(),
        agent_forwarding: session.agent_forwarding,
    };
    connecting.store(true, Ordering::Relaxed);
    window.refresh();
    let cancellation = connect_ssh_session(
        cfg,
        SshConnectRequest {
            logging_override: session.logging,
            on_connected: Some(on_connected),
            on_failed: Some({
                let auth_form = auth_form.clone();
                let hop_forms = hop_forms.clone();
                let session = session.clone();
                Rc::new(move |error, message, window, cx| {
                    inline_error.set(message);
                    if let Some(rejected) =
                        reveal_rejected_saved_password(error, &auth_form, &hop_forms)
                    {
                        remember_rejection(rejected, id, &session, window, cx);
                    }
                })
            }),
            ..SshConnectRequest::new(session.label.clone())
        },
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

    fn saved(host: &str) -> SavedPassword {
        SavedPassword {
            sealed: serde_json::from_value(serde_json::Value::String("otv1:AAAA".into())).unwrap(),
            host: host.into(),
            port: 22,
            username: "ops".into(),
        }
    }

    fn auth_error(message: &str) -> AppError {
        AppError::Connect {
            phase: ConnectPhase::Authentication,
            message: message.into(),
        }
    }

    /// `ssh-password-save-flow.md` § 4 and 6: a rejected saved password turns
    /// only its own row (hop or target) into an input; other failures leave
    /// the rows alone; with no unlocked vault a saved row opens nothing and
    /// asks for the password instead.
    #[gpui::test]
    fn a_rejected_saved_password_reveals_only_its_own_row(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let probe = Rc::new(RefCell::new(None));
        let probe_for_window = probe.clone();
        let (_view, cx) = cx.add_window_view(move |window, cx| {
            let target = SshAuthForm::new(SshAuthPreference::Password, None, window, cx);
            target.set_saved(Some(saved("10.0.0.1")));
            let hops = JumpHopForms::new(
                vec![HopSpec {
                    label: "bastion".into(),
                    host: "10.0.0.2".into(),
                    port: 22,
                    username: "ops".into(),
                    auth_method: SshAuthPreference::Password,
                    key_path: None,
                    saved: Some(saved("10.0.0.2")),
                }],
                window,
                cx,
            );
            let mut forms: Vec<SshAuthForm> = hops.forms().cloned().collect();
            forms.push(target.clone());
            for form in &forms {
                form.use_saved(true);
            }
            *probe_for_window.borrow_mut() = Some((target, hops));
            FocusTestView {
                initial: cx.focus_handle(),
                user_selected: cx.focus_handle(),
            }
        });
        let cx: &mut VisualTestContext = cx;
        let (target, hops) = probe.borrow().clone().unwrap();
        let hop = hops.forms().next().unwrap().clone();
        assert!(target.uses_saved() && hop.uses_saved());

        // A network failure is not a rejection.
        let transport = AppError::Connect {
            phase: ConnectPhase::Transport,
            message: "timed out".into(),
        };
        assert_eq!(
            reveal_rejected_saved_password(&transport, &target, &hops),
            None
        );
        assert!(target.uses_saved() && hop.uses_saved());

        // The hop's own address picks the hop; the target keeps its row.
        assert_eq!(
            reveal_rejected_saved_password(
                &auth_error("jump host ops@10.0.0.2:22: the server rejected the password"),
                &target,
                &hops,
            ),
            Some(Rejected::Hop)
        );
        assert!(!hop.uses_saved() && !hop.save_ticked());
        assert!(target.uses_saved());

        // Otherwise it is the target's, with "Update saved password" ticked.
        assert_eq!(
            reveal_rejected_saved_password(
                &auth_error("the server rejected the password"),
                &target,
                &hops,
            ),
            Some(Rejected::Target)
        );
        assert!(!target.uses_saved());
        assert!(target.save_ticked());
        // A row already revealed is not reported twice.
        assert_eq!(
            reveal_rejected_saved_password(
                &auth_error("the server rejected the password"),
                &target,
                &hops,
            ),
            None
        );

        // With no unlocked vault the saved row opens nothing.
        target.use_saved(true);
        let auth = cx.update(|_, cx| target.take_auth(cx));
        assert!(auth.unwrap_err().contains("locked"));
        assert!(!target.uses_saved(), "the input is revealed instead");
    }

    /// `US-0159` rework: deleting `vault.json` by hand (the lock screen's
    /// recovery path) leaves envelopes in `ssh_session.json`. With no vault a
    /// saved row asks for the password and says why, keeps its envelope until
    /// a new one replaces it, and asks for no unlock.
    #[gpui::test]
    fn a_saved_password_outliving_its_vault_asks_for_the_password(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        // Never created: `vault.json` is missing, so the vault is Uninitialised.
        let gone = std::env::temp_dir().join(format!(
            "oneterm-deleted-vault-{}-never-created",
            std::process::id()
        ));
        cx.update(|cx| {
            let vault = cx.new(|_| VaultState::load_at_start(gone, false));
            cx.set_global(oneterm_state::vault_state::VaultStateGlobal(vault));
        });
        let probe = Rc::new(RefCell::new(None));
        let probe_for_window = probe.clone();
        let (_view, cx) = cx.add_window_view(move |window, cx| {
            let form = SshAuthForm::new(SshAuthPreference::Password, None, window, cx);
            form.set_saved(Some(saved("10.0.0.1")));
            *probe_for_window.borrow_mut() = Some(form);
            FocusTestView {
                initial: cx.focus_handle(),
                user_selected: cx.focus_handle(),
            }
        });
        let cx: &mut VisualTestContext = cx;
        let form = probe.borrow().clone().unwrap();
        let unlock_first =
            cx.update(|_, cx| activate_saved_passwords(std::slice::from_ref(&form), cx));
        assert!(!unlock_first, "an unlock was asked for with no vault");
        assert!(
            !form.uses_saved(),
            "the saved row stayed in place of the input"
        );
        assert!(form.has_saved(), "the envelope is kept until replaced");
        assert!(
            form.note()
                .unwrap()
                .contains("the master password was reset")
        );
        // Asked for anyway, it opens nothing and leaves the input.
        form.use_saved(true);
        let auth = cx.update(|_, cx| form.take_auth(cx));
        assert!(auth.is_err());
        assert!(!form.uses_saved());
    }

    /// Rework D2: a rejection remembered for a session opens its next connect
    /// dialog with the input revealed, but only while the session still holds
    /// the envelope that was rejected (a new one saved meanwhile clears it).
    #[gpui::test]
    fn a_remembered_rejection_follows_the_envelope_it_was_for(cx: &mut TestAppContext) {
        let envelope = |text: &str| -> SealedSecret {
            serde_json::from_value(serde_json::Value::String(text.into())).unwrap()
        };
        let id = SshSessionId::parse("4").unwrap();
        let mut session: SshSession = serde_json::from_str(
            r#"{ "label": "prod", "host": "10.0.0.1", "username": "deploy", "group": null,
                 "sealed_password": "otv1:AAAA" }"#,
        )
        .unwrap();
        cx.update(|cx| {
            assert!(!was_rejected(id, &session, cx), "nothing remembered yet");
            cx.default_global::<RejectedSavedPasswords>()
                .0
                .insert(id, envelope("otv1:AAAA"));
            assert!(was_rejected(id, &session, cx));
            assert!(!was_rejected(
                SshSessionId::parse("5").unwrap(),
                &session,
                cx
            ));
            session.sealed_password = Some(envelope("otv1:BBBB"));
            assert!(!was_rejected(id, &session, cx), "a new password was saved");
            session.sealed_password = None;
            assert!(!was_rejected(id, &session, cx), "the password was removed");
        });
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
