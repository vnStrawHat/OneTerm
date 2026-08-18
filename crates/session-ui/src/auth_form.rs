//! Shared SSH authentication form state and private-key validation.

use std::cell::Cell;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _,
    PathPromptOptions, Styled, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    input::{Input, InputState},
    radio::RadioGroup,
    v_flex,
};
use oneterm_core::{SecretString, SshAuthMethod};
use oneterm_state::form_dialog::{FieldRequirement, labelled_field};

use crate::session_state::SshAuthPreference;

/// UI state shared by saved-session and Quick Connect authentication forms.
#[derive(Clone)]
pub(crate) struct SshAuthForm {
    method: Rc<Cell<SshAuthPreference>>,
    password: gpui::Entity<InputState>,
    key_path: gpui::Entity<InputState>,
    passphrase: gpui::Entity<InputState>,
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
        }
    }

    pub(crate) fn method(&self) -> SshAuthPreference {
        self.method.get()
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        match self.method() {
            SshAuthPreference::Password => self.password.read(cx).focus_handle(cx),
            SshAuthPreference::PrivateKey => self.key_path.read(cx).focus_handle(cx),
        }
    }

    /// Focus the credential field appropriate for the selected method.
    pub(crate) fn secret_focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        match self.method() {
            SshAuthPreference::Password => self.password.read(cx).focus_handle(cx),
            SshAuthPreference::PrivateKey => self.passphrase.read(cx).focus_handle(cx),
        }
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
                    .children(["Password", "Private Key"])
                    .selected_index(Some(selected_index))
                    .on_click(move |selected: &usize, window, cx| {
                        let private_key_selected = *selected == 1;
                        method.set(if private_key_selected {
                            SshAuthPreference::PrivateKey
                        } else {
                            SshAuthPreference::Password
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
                |form| {
                    form.child(v_flex().id("password-auth-fields").child(labelled_field(
                        "Password",
                        FieldRequirement::Optional,
                        Input::new(&self.password).mask_toggle().cleanable(true),
                        cx,
                    )))
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

    /// Build the backend authentication config and clear credential input state.
    pub(crate) fn take_auth(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<SshAuthMethod, String> {
        match self.method() {
            SshAuthPreference::Password => {
                let password = self.password.read(cx).value().to_string();
                self.password
                    .update(cx, |state, cx| state.set_value("", window, cx));
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
                self.passphrase
                    .update(cx, |state, cx| state.set_value("", window, cx));
                Ok(SshAuthMethod::PrivateKey {
                    key_path,
                    passphrase: (!passphrase.is_empty()).then(|| SecretString::new(passphrase)),
                })
            }
        }
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
        let secret_focus = view.read_with(cx, |view, cx| view.form.secret_focus_handle(cx));
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
        let secret_focus = view.read_with(cx, |view, cx| view.form.secret_focus_handle(cx));
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

    #[test]
    fn private_key_validation_accepts_readable_file() {
        let path = std::env::temp_dir().join(format!(
            "oneterm-private-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::write(&path, "not-a-real-key-but-readable").unwrap();

        assert_eq!(
            validate_private_key_path(path.to_str().unwrap()),
            Ok(path.clone())
        );

        std::fs::remove_file(path).unwrap();
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
