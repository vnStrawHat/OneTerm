//! Shared SSH authentication form state and private-key validation.
//!
//! The password half can also stand for a **saved** password (`US-0158`,
//! `ssh-password-save-flow.md`): the form then shows "Using saved password"
//! instead of the input and opens the envelope only in [`SshAuthForm::take_auth`],
//! at the moment of connecting, so nothing decrypted lives in a widget.

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, FocusHandle, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, PathPromptOptions, SharedString, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Icon, Sizable as _,
    button::Button,
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    v_flex,
};
use oneterm_core::{SecretString, SshAuthMethod};
use oneterm_state::VaultState;
use oneterm_state::form_dialog::{FieldRequirement, control_label, labelled_field};
use oneterm_theme::icon::AppIcon;
use oneterm_vault::{OpenError, SealContext, SealedSecret, SecretPurpose};

use crate::session_state::{SshAuthPreference, SshSession};

/// Label of the checkbox that saves a typed password.
const SAVE_PASSWORD_LABEL: &str = "Save password (encrypted with your master password)";
/// The same checkbox when the session already has a saved password.
const UPDATE_PASSWORD_LABEL: &str = "Update saved password";

/// A saved password and the address it is bound to (`DEC-0021`, intake Q3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedPassword {
    pub(crate) sealed: SealedSecret,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) username: String,
}

impl SavedPassword {
    /// `session`'s saved password with its address, when it authenticates by
    /// password and has both a saved password and a username.
    pub(crate) fn of(session: &SshSession) -> Option<Self> {
        let sealed = session.saved_password()?.clone();
        let context = session.seal_context()?;
        Some(Self {
            sealed,
            host: context.host.to_owned(),
            port: context.port,
            username: context.username.to_owned(),
        })
    }

    pub(crate) fn context(&self) -> SealContext<'_> {
        SealContext {
            host: &self.host,
            port: self.port,
            username: &self.username,
        }
    }
}

/// Why a saved password cannot be opened, as the row says it.
pub(crate) fn open_failure(error: OpenError) -> String {
    let reason = match error {
        OpenError::OtherVault => "it was saved under a master password that was reset",
        OpenError::NewerFormat => "it was saved by a newer OneTerm",
        OpenError::Malformed => "the stored value is not a saved password",
        OpenError::Rejected => "it cannot be used for this address",
    };
    format!("The saved password cannot be used: {reason}.")
}

/// UI state shared by saved-session and Quick Connect authentication forms.
#[derive(Clone)]
pub(crate) struct SshAuthForm {
    method: Rc<Cell<SshAuthPreference>>,
    password: gpui::Entity<InputState>,
    key_path: gpui::Entity<InputState>,
    passphrase: gpui::Entity<InputState>,
    /// The saved password this form may use instead of the input.
    saved: Rc<RefCell<Option<SavedPassword>>>,
    /// The saved password is in use (the input is hidden).
    use_saved: Rc<Cell<bool>>,
    /// "Save password" / "Update saved password" is ticked.
    save_ticked: Rc<Cell<bool>>,
    /// The session dialog's **Remove** was clicked: the saved password goes
    /// on Save. Only Remove sets it; unticking "Update saved password" after
    /// Replace keeps the saved password.
    removed: Rc<Cell<bool>>,
    /// Whether a typed password may be saved from this form.
    offers_save: Rc<Cell<bool>>,
    /// Why the saved password is not in use, shown under the input.
    note: Rc<RefCell<Option<SharedString>>>,
    /// The "Using saved password" row: focusable, so Enter submits from it.
    saved_focus: FocusHandle,
}

impl SshAuthForm {
    pub(crate) fn new(
        method: SshAuthPreference,
        key_path: Option<&Path>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter password")
                .masked(true)
        });
        let key_path_state = cx.new(|cx| {
            let mut state =
                InputState::new(window, cx).placeholder("Select or enter private key path");
            if let Some(path) = key_path {
                state.set_value(path.to_string_lossy().into_owned(), window, cx);
            }
            state
        });
        let passphrase = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Enter passphrase if the key is encrypted")
                .masked(true)
        });

        Self {
            method: Rc::new(Cell::new(method)),
            password,
            key_path: key_path_state,
            passphrase,
            saved: Rc::new(RefCell::new(None)),
            use_saved: Rc::new(Cell::new(false)),
            save_ticked: Rc::new(Cell::new(false)),
            removed: Rc::new(Cell::new(false)),
            offers_save: Rc::new(Cell::new(false)),
            note: Rc::new(RefCell::new(None)),
            saved_focus: cx.focus_handle(),
        }
    }

    pub(crate) fn method(&self) -> SshAuthPreference {
        self.method.get()
    }

    /// Give the form a saved password. It is not used until
    /// [`Self::use_saved`] says so (the vault may still be locked).
    pub(crate) fn set_saved(&self, saved: Option<SavedPassword>) {
        *self.saved.borrow_mut() = saved;
    }

    pub(crate) fn has_saved(&self) -> bool {
        self.method() == SshAuthPreference::Password && self.saved.borrow().is_some()
    }

    /// Show "Using saved password" instead of the input (no-op without one).
    pub(crate) fn use_saved(&self, on: bool) {
        self.use_saved.set(on && self.has_saved());
        if on {
            self.note.borrow_mut().take();
        }
    }

    pub(crate) fn uses_saved(&self) -> bool {
        self.use_saved.get() && self.has_saved()
    }

    /// Offer "Save password" under a typed password.
    pub(crate) fn offer_save(&self, on: bool) {
        self.offers_save.set(on);
    }

    /// The user asked to save the password typed in this form.
    pub(crate) fn save_ticked(&self) -> bool {
        self.save_ticked.get() && self.method() == SshAuthPreference::Password && !self.uses_saved()
    }

    /// A muted line under the password input (why the saved one is not used).
    pub(crate) fn set_note(&self, note: impl Into<SharedString>) {
        *self.note.borrow_mut() = Some(note.into());
    }

    /// Swap the saved row back to an empty input, saying why; `tick` also
    /// ticks "Update saved password" (a rejected saved password).
    pub(crate) fn reveal_input(&self, note: impl Into<SharedString>, tick: bool) {
        self.use_saved.set(false);
        self.save_ticked.set(tick);
        self.set_note(note);
    }

    /// Whether Save keeps the saved password (session dialog): it exists, was
    /// not removed, and no typed password replaces it. Replace followed by
    /// unticking "Update saved password" keeps it.
    pub(crate) fn keeps_saved(&self) -> bool {
        self.has_saved() && !self.removed.get() && !self.save_ticked()
    }

    /// The session dialog's **Replace**: an empty input with "Update saved
    /// password" ticked; unticking it again keeps the saved password.
    pub(crate) fn begin_replace(&self) {
        self.use_saved.set(false);
        self.save_ticked.set(true);
        self.removed.set(false);
    }

    /// Tick or untick "Save password" / "Update saved password".
    pub(crate) fn set_save_ticked(&self, on: bool) {
        self.save_ticked.set(on);
    }

    /// The session dialog's **Remove**: the saved password is dropped on Save.
    pub(crate) fn remove_saved(&self) {
        self.reveal_input("The saved password is removed when you save.", false);
        self.removed.set(true);
    }

    /// Whether the password input is empty (copies nothing).
    pub(crate) fn typed_is_empty(&self, cx: &App) -> bool {
        self.password.read(cx).text().len_chars() == 0
    }

    /// The typed password, copied once into a zeroizing string.
    pub(crate) fn typed_password(&self, cx: &App) -> SecretString {
        SecretString::new(self.password.read(cx).text().to_string())
    }

    /// The first input of the selected method; `None` when the method has no
    /// input (SSH agent). A saved password's row stands for its input.
    pub(crate) fn focus_handle(&self, cx: &App) -> Option<gpui::FocusHandle> {
        match self.method() {
            SshAuthPreference::Password if self.uses_saved() => Some(self.saved_focus.clone()),
            SshAuthPreference::Password => Some(self.password.read(cx).focus_handle(cx)),
            SshAuthPreference::PrivateKey => Some(self.key_path.read(cx).focus_handle(cx)),
            SshAuthPreference::Agent => None,
        }
    }

    /// The credential field appropriate for the selected method; `None` when
    /// the method asks for no secret (SSH agent) or a saved password stands in.
    pub(crate) fn secret_focus_handle(&self, cx: &App) -> Option<gpui::FocusHandle> {
        match self.method() {
            SshAuthPreference::Password if self.uses_saved() => None,
            SshAuthPreference::Password => Some(self.password.read(cx).focus_handle(cx)),
            SshAuthPreference::PrivateKey => Some(self.passphrase.read(cx).focus_handle(cx)),
            SshAuthPreference::Agent => None,
        }
    }

    /// The "Using saved password" row, while it is shown.
    pub(crate) fn saved_focus_handle(&self) -> Option<gpui::FocusHandle> {
        self.uses_saved().then(|| self.saved_focus.clone())
    }

    /// Open the saved password now, for this connection only. A failure turns
    /// the row into an empty input with the reason, and nothing is sent.
    fn open_saved(&self, cx: &App) -> Result<SecretString, String> {
        let Some(saved) = self.saved.borrow().clone() else {
            return Err("No saved password.".to_owned());
        };
        let opened = VaultState::try_global(cx).and_then(|state| {
            state.read(cx).unlocked().map(|vault| {
                vault.open(SecretPurpose::SshPassword, &saved.context(), &saved.sealed)
            })
        });
        let reason = match opened {
            Some(Ok(password)) => return Ok(password),
            Some(Err(error)) => open_failure(error),
            None => "Saved passwords are locked.".to_owned(),
        };
        let message = format!("{reason} Type the password.");
        self.reveal_input(message.clone(), false);
        Err(message)
    }

    /// The input, or the "Using saved password" row, of the Password method.
    fn render_password(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        if self.uses_saved() {
            let form = self.clone();
            return labelled_field(
                "Password",
                FieldRequirement::Optional,
                h_flex()
                    .id("saved-password-row")
                    .track_focus(&self.saved_focus)
                    .gap_2()
                    .items_center()
                    .w_full()
                    .child(Icon::new(AppIcon::Lock).small().text_color(muted))
                    .child(div().flex_1().text_sm().child("Using saved password"))
                    .child(
                        Button::new("use-different-password")
                            .label("Use a different one")
                            .outline()
                            .small()
                            .on_click(move |_, window, cx| {
                                form.use_saved.set(false);
                                window.refresh();
                                let focus = form.password.read(cx).focus_handle(cx);
                                window.defer(cx, move |window, cx| focus.focus(window, cx));
                            }),
                    ),
                cx,
            )
            .into_any_element();
        }
        v_flex()
            .id("password-auth-fields")
            .gap_2()
            .w_full()
            .child(labelled_field(
                "Password",
                FieldRequirement::Optional,
                Input::new(&self.password).mask_toggle().cleanable(true),
                cx,
            ))
            .when(self.offers_save.get(), |this| {
                this.child(self.save_checkbox())
            })
            .when_some(self.note.borrow().clone(), |this, note| {
                this.child(div().text_xs().text_color(muted).child(note))
            })
            .into_any_element()
    }

    fn save_checkbox(&self) -> impl IntoElement {
        let label = if self.saved.borrow().is_some() {
            UPDATE_PASSWORD_LABEL
        } else {
            SAVE_PASSWORD_LABEL
        };
        let form = self.clone();
        Checkbox::new("save-password")
            .items_center()
            .accessibility_label(label)
            .child(control_label(label))
            .checked(self.save_ticked.get())
            .on_click(move |checked: &bool, window, _| {
                form.set_save_ticked(*checked);
                window.refresh();
            })
    }

    /// The session dialog's password row (`US-0158`): with a saved password,
    /// "Saved password" + Replace / Remove (it is never shown); otherwise the
    /// input + "Save password". Only for the Password method.
    pub(crate) fn render_saved_password_editor(&self, cx: &App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        if self.method() != SshAuthPreference::Password {
            return div().into_any_element();
        }
        if !self.uses_saved() {
            return v_flex()
                .id("password-save-fields")
                .gap_2()
                .w_full()
                .child(labelled_field(
                    "Password",
                    FieldRequirement::Optional,
                    Input::new(&self.password).mask_toggle().cleanable(true),
                    cx,
                ))
                .child(self.save_checkbox())
                .when_some(self.note.borrow().clone(), |this, note| {
                    this.child(div().text_xs().text_color(muted).child(note))
                })
                .into_any_element();
        }
        let replace = self.clone();
        let remove = self.clone();
        labelled_field(
            "Password",
            FieldRequirement::Optional,
            h_flex()
                .id("saved-password-editor")
                .gap_2()
                .items_center()
                .w_full()
                .child(Icon::new(AppIcon::Lock).small().text_color(muted))
                .child(div().flex_1().text_sm().child("Saved password"))
                .child(
                    Button::new("replace-saved-password")
                        .label("Replace")
                        .outline()
                        .small()
                        .on_click(move |_, window, cx| {
                            replace.begin_replace();
                            window.refresh();
                            let focus = replace.password.read(cx).focus_handle(cx);
                            window.defer(cx, move |window, cx| focus.focus(window, cx));
                        }),
                )
                .child(
                    Button::new("remove-saved-password")
                        .label("Remove")
                        .outline()
                        .small()
                        .on_click(move |_, window, _| {
                            remove.remove_saved();
                            window.refresh();
                        }),
                ),
            cx,
        )
        .into_any_element()
    }

    pub(crate) fn key_path_value(&self, cx: &App) -> Option<PathBuf> {
        let value = self.key_path.read(cx).value().trim().to_string();
        (!value.is_empty()).then(|| PathBuf::from(value))
    }

    /// Render the authentication fields.
    ///
    /// `show_secrets` adds the connect-time credential inputs (password /
    /// passphrase); without it only the persisted metadata is shown.
    pub(crate) fn render(&self, show_secrets: bool, cx: &App) -> impl IntoElement {
        let selected_index = match self.method() {
            SshAuthPreference::Password => 0,
            SshAuthPreference::PrivateKey => 1,
            SshAuthPreference::Agent => 2,
        };
        let method = self.method.clone();
        let passphrase_for_selection = self.passphrase.clone();

        v_flex()
            .gap_3()
            .w_full()
            .child(labelled_field(
                "Authentication",
                FieldRequirement::Required,
                RadioGroup::horizontal("ssh-auth-method")
                    // `control_label` instead of `Radio::from(&str)`, whose
                    // `.label(…)` clips the `y` of "Private Key" (BUG-0069);
                    // `items_center` because that taller label box would
                    // otherwise hang below the radio's indicator.
                    .children(["Password", "Private Key", "SSH Agent"].map(|label| {
                        Radio::new(label)
                            .items_center()
                            .accessibility_label(label)
                            .child(control_label(label))
                    }))
                    .selected_index(Some(selected_index))
                    .on_click(move |selected: &usize, window, cx| {
                        let private_key_selected = *selected == 1;
                        method.set(match *selected {
                            1 => SshAuthPreference::PrivateKey,
                            2 => SshAuthPreference::Agent,
                            _ => SshAuthPreference::Password,
                        });
                        window.refresh();
                        if private_key_selected && show_secrets {
                            let passphrase = passphrase_for_selection.clone();
                            window.defer(cx, move |window, cx| {
                                passphrase.read(cx).focus_handle(cx).focus(window, cx);
                            });
                        }
                    }),
                cx,
            ))
            .when(
                show_secrets && self.method() == SshAuthPreference::Password,
                |form| form.child(self.render_password(cx)),
            )
            .when(
                show_secrets && self.method() == SshAuthPreference::Agent,
                |form| {
                    form.child(
                        v_flex()
                            .id("agent-auth-fields")
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Keys come from the running SSH agent; nothing to enter."),
                    )
                },
            )
            .when(self.method() == SshAuthPreference::PrivateKey, |form| {
                let key_path = self.key_path.clone();
                form.child(
                    v_flex()
                        .id("private-key-auth-fields")
                        .gap_3()
                        .w_full()
                        .child(labelled_field(
                            "Private Key",
                            FieldRequirement::Required,
                            h_flex()
                                .gap_2()
                                .w_full()
                                .child(Input::new(&self.key_path).flex_1().cleanable(true))
                                .child(
                                    Button::new("browse-private-key")
                                        .label("Browse")
                                        .outline()
                                        .on_click({
                                            let passphrase = self.passphrase.clone();
                                            move |_, window, cx| {
                                                browse_for_private_key(
                                                    key_path.clone(),
                                                    show_secrets.then(|| passphrase.clone()),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        }),
                                ),
                            cx,
                        ))
                        .when(show_secrets, |form| {
                            form.child(labelled_field(
                                "Passphrase",
                                FieldRequirement::Optional,
                                Input::new(&self.passphrase).mask_toggle().cleanable(true),
                                cx,
                            ))
                        }),
                )
            })
    }

    /// Build the backend authentication config from the credential fields.
    ///
    /// The fields are **not** cleared. They used to be, which meant a connect
    /// that failed 20 s later left an empty password box and a re-enabled
    /// button, and every retry cost a full re-type (`F4`). The secret now lives
    /// as long as the dialog does and no longer: a successful connect closes the
    /// dialog, and closing it drops this form and its `InputState` entities.
    /// Nothing on either path writes the value to `ssh_session.json`, to any
    /// other config file or to the log — `DEC-0001` / `docs/ssh-client-connect.md`
    /// §1.3 decision 1 stands, because keeping a secret in an open modal's own
    /// state is not persistence.
    ///
    /// A saved password (`US-0158`) is opened here, at the moment of
    /// connecting, into a fresh zeroizing string; nothing decrypted is kept in
    /// the form.
    pub(crate) fn take_auth(&self, cx: &App) -> Result<SshAuthMethod, String> {
        match self.method() {
            SshAuthPreference::Password if self.uses_saved() => self
                .open_saved(cx)
                .map(|password| SshAuthMethod::Password { password }),
            SshAuthPreference::Password => {
                let password = self.password.read(cx).value().to_string();
                if password.is_empty() {
                    Ok(SshAuthMethod::None)
                } else {
                    Ok(SshAuthMethod::Password {
                        password: SecretString::new(password),
                    })
                }
            }
            SshAuthPreference::PrivateKey => {
                let key_path = self.key_path.read(cx).value().trim().to_string();
                let key_path = validate_private_key_path(&key_path)?;
                let passphrase = self.passphrase.read(cx).value().to_string();
                Ok(SshAuthMethod::PrivateKey {
                    key_path,
                    passphrase: (!passphrase.is_empty()).then(|| SecretString::new(passphrase)),
                })
            }
            SshAuthPreference::Agent => Ok(SshAuthMethod::Agent),
        }
    }

    /// The credential inputs, so a caller can clear an inline error when the
    /// user starts correcting one of them.
    pub(crate) fn secret_inputs(&self) -> Vec<gpui::Entity<InputState>> {
        vec![self.password.clone(), self.passphrase.clone()]
    }
}

fn browse_for_private_key(
    key_path: gpui::Entity<InputState>,
    focus_after: Option<gpui::Entity<InputState>>,
    window: &mut Window,
    cx: &mut App,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select an SSH private key".into()),
    });

    window
        .spawn(cx, async move |cx| {
            let selected = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    log::warn!("failed to open SSH private-key picker: {error}");
                    return;
                }
                Err(error) => {
                    log::warn!("SSH private-key picker response was dropped: {error}");
                    return;
                }
            };
            let Some(selected) = selected else {
                return;
            };
            let value = selected.to_string_lossy().into_owned();
            _ = cx.update(|window, cx| {
                key_path.update(cx, |state, cx| state.set_value(value, window, cx));
                if let Some(focus_after) = focus_after {
                    window.defer(cx, move |window, cx| {
                        focus_after.read(cx).focus_handle(cx).focus(window, cx);
                    });
                }
            });
        })
        .detach();
}

fn validate_private_key_path(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("Private key path is required.".to_string());
    }

    let path = PathBuf::from(raw.trim());
    let metadata = std::fs::metadata(&path)
        .map_err(|error| format!("Private key file is unavailable: {error}"))?;
    if !metadata.is_file() {
        return Err("Private key path must point to a file.".to_string());
    }
    File::open(&path).map_err(|error| format!("Private key file is not readable: {error}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::{Context, Modifiers, Render, TestAppContext, VisualTestContext, point, px};
    use gpui_component::Root;

    use super::*;

    struct AuthFormTestView {
        form: SshAuthForm,
    }

    /// Removes the fixture file on drop, including when an assertion panics
    /// (`Drop` runs during unwind): a manual `std::fs::remove_file` as a
    /// test's last statement never runs if an earlier assertion in the same
    /// test panics (`BUG-0083`, same latent-leak shape `crash_report.rs`'s
    /// `TemporaryDirectory` fixed for `BUG-0082`).
    struct TemporaryFile(PathBuf);

    impl Drop for TemporaryFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    impl std::ops::Deref for TemporaryFile {
        type Target = Path;

        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Render for AuthFormTestView {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            gpui::div()
                .w(gpui::px(440.))
                .p_4()
                .child(self.form.render(true, cx))
        }
    }

    #[gpui::test]
    fn secret_focus_targets_password_for_password_auth(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let view_probe = Rc::new(RefCell::new(None));
        let probe_for_window = view_probe.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AuthFormTestView {
                form: SshAuthForm::new(SshAuthPreference::Password, None, window, cx),
            });
            *probe_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let view = view_probe
            .borrow()
            .clone()
            .expect("view must be initialized");
        let secret_focus = view
            .read_with(cx, |view, cx| view.form.secret_focus_handle(cx))
            .expect("password auth has a secret field");
        cx.update(|window, cx| secret_focus.focus(window, cx));
        cx.run_until_parked();

        assert!(cx.update(|window, _| secret_focus.is_focused(window)));
        assert!(cx.update(|window, cx| {
            view.read(cx)
                .form
                .password
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        }));
    }

    #[gpui::test]
    fn secret_focus_targets_passphrase_for_private_key_auth(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let view_probe = Rc::new(RefCell::new(None));
        let probe_for_window = view_probe.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AuthFormTestView {
                form: SshAuthForm::new(SshAuthPreference::PrivateKey, None, window, cx),
            });
            *probe_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let view = view_probe
            .borrow()
            .clone()
            .expect("view must be initialized");
        let secret_focus = view
            .read_with(cx, |view, cx| view.form.secret_focus_handle(cx))
            .expect("private key auth has a secret field");
        cx.update(|window, cx| secret_focus.focus(window, cx));
        cx.run_until_parked();

        assert!(cx.update(|window, _| secret_focus.is_focused(window)));
        assert!(cx.update(|window, cx| {
            view.read(cx)
                .form
                .passphrase
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        }));
    }

    #[gpui::test]
    fn passphrase_input_accepts_pointer_focus_and_keyboard_input(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let passphrase_probe = Rc::new(RefCell::new(None));
        let probe_for_window = passphrase_probe.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AuthFormTestView {
                form: SshAuthForm::new(SshAuthPreference::PrivateKey, None, window, cx),
            });
            *probe_for_window.borrow_mut() = Some(view.read(cx).form.passphrase.clone());
            Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();

        cx.simulate_click(point(px(100.), px(185.)), Modifiers::none());
        cx.run_until_parked();

        let passphrase = passphrase_probe
            .borrow()
            .clone()
            .expect("passphrase probe must be initialized");
        let passphrase_is_focused =
            cx.update(|window, cx| passphrase.read(cx).focus_handle(cx).is_focused(window));
        assert!(
            passphrase_is_focused,
            "passphrase input must own focus after a pointer click"
        );

        cx.simulate_input("test-passphrase");
        cx.run_until_parked();
        assert_eq!(
            passphrase.read_with(cx, |state, _| state.value().to_string()),
            "test-passphrase",
            "focused passphrase input must accept keyboard text",
        );
    }

    #[gpui::test]
    fn agent_auth_has_no_input_to_focus_and_takes_agent_auth(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let view_probe = Rc::new(RefCell::new(None));
        let probe_for_window = view_probe.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            let view = cx.new(|cx| AuthFormTestView {
                form: SshAuthForm::new(SshAuthPreference::Agent, None, window, cx),
            });
            *probe_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let cx: &mut VisualTestContext = cx;
        let view = view_probe
            .borrow()
            .clone()
            .expect("view must be initialized");

        assert!(view.read_with(cx, |view, cx| view.form.focus_handle(cx).is_none()));
        assert!(view.read_with(cx, |view, cx| view.form.secret_focus_handle(cx).is_none()));
        let form = view.read_with(cx, |view, _| view.form.clone());
        let auth = cx.update(|_, cx| form.take_auth(cx));
        assert!(matches!(auth, Ok(SshAuthMethod::Agent)), "{auth:?}");
    }

    #[test]
    fn private_key_validation_accepts_readable_file() {
        let path = TemporaryFile(std::env::temp_dir().join(format!(
            "oneterm-private-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        std::fs::write(&*path, "not-a-real-key-but-readable").unwrap();

        assert_eq!(
            validate_private_key_path(path.to_str().unwrap()),
            Ok(path.to_path_buf())
        );
    }

    #[test]
    fn private_key_validation_rejects_missing_and_directory_paths() {
        assert_eq!(
            validate_private_key_path(""),
            Err("Private key path is required.".to_string())
        );
        assert!(validate_private_key_path("missing-oneterm-private-key").is_err());
        assert_eq!(
            validate_private_key_path(std::env::temp_dir().to_str().unwrap()),
            Err("Private key path must point to a file.".to_string())
        );
    }
}
