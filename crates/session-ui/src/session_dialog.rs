//! "New / Edit SSH Session" dialog — create or edit an SSH session.
//!
//! Built on `oneterm_state::form_dialog::FormDialog` (Cancel + Save footer,
//! Enter submits). On Save → validate (Label & Host required, Port numeric) →
//! `store.add` (create) or `store.update(id, …)` (edit) → auto-saves
//! `ssh_session.json`. An edit addresses the session by its stable id, so a
//! session removed or reordered while the dialog is open is never mistaken
//! for another one.
//!
//! Form fields: Label, Host, Port, Username, authentication preference, and Group.
//!
//! The Group field uses a [`Combobox`] with `searchable(true)` + a "Create" footer —
//! the user can **pick an existing group** or **type a new one**.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{App, AppContext, Hsla, ParentElement as _, SharedString, Styled, Window};
use gpui_component::{
    ActiveTheme, Colorize as _, IndexPath, Sizable as _, WindowExt as _,
    color_picker::{ColorPicker, ColorPickerState},
    combobox::ComboboxState,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    radio::Radio,
};

use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_theme::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::parse_port;
use super::group_combo::{GroupComboDelegate, SharedCell, group_combobox};
use crate::session_state::{
    SshAuthPreference, SshLoggingOverride, SshSession, SshSessionEntry, SshSessionId,
    SshSessionStore,
};

/// The raw field values of the session form, as typed by the user.
#[derive(Debug, Clone, Default, PartialEq)]
struct SessionForm {
    label: String,
    host: String,
    /// Empty means the default port.
    port: String,
    username: String,
    group: String,
    color: Option<String>,
    auth_method: SshAuthPreference,
    /// The private-key path, when the auth form has a valid one.
    key_path: Option<PathBuf>,
    logging: SshLoggingOverride,
}

impl SessionForm {
    /// Validate the form and build the session to store. `Err` carries the
    /// message shown to the user (Label & Host required, Port numeric, private
    /// key required for key auth); optional fields fall back to `None`.
    fn into_session(self) -> Result<SshSession, String> {
        let label = self.label.trim().to_string();
        let host = self.host.trim().to_string();
        if label.is_empty() || host.is_empty() {
            return Err("Label and Host are required.".to_string());
        }
        let port_text = self.port.trim();
        let port = if port_text.is_empty() {
            SshSession::DEFAULT_PORT
        } else {
            parse_port(port_text).map_err(|error| error.to_string())?
        };
        let key_path = match self.auth_method {
            SshAuthPreference::PrivateKey => Some(
                self.key_path
                    .ok_or_else(|| "Private key path is required.".to_string())?,
            ),
            SshAuthPreference::Password => None,
        };
        Ok(SshSession {
            label,
            host,
            port,
            username: non_empty(self.username),
            auth_method: self.auth_method,
            key_path,
            color: self.color,
            group: non_empty(self.group),
            logging: self.logging,
        })
    }
}

/// `None` for blank input, the trimmed text otherwise.
fn non_empty(text: String) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The distinct, trimmed, sorted group names in use — the combobox choices.
fn existing_group_names(sessions: &[SshSessionEntry]) -> Vec<SharedString> {
    let mut groups: Vec<String> = sessions
        .iter()
        .filter_map(|entry| entry.session.group.as_ref().map(|g| g.trim().to_string()))
        .filter(|g| !g.is_empty())
        .collect();
    groups.sort();
    groups.dedup();
    groups.into_iter().map(SharedString::from).collect()
}

fn logging_radio(
    id: &'static str,
    label: &'static str,
    value: SshLoggingOverride,
    selected: Rc<Cell<SshLoggingOverride>>,
) -> Radio {
    Radio::new(id)
        .label(label)
        .checked(selected.get() == value)
        .on_click(move |checked, window, _| {
            if *checked {
                selected.set(value);
                window.refresh();
            }
        })
}

/// Open the dialog to create (when `edit` = `None`) or edit (when `edit` =
/// `Some((id, session))`) an SSH session.
pub(crate) fn open_session_dialog(
    window: &mut Window,
    cx: &mut App,
    edit: Option<(SshSessionId, SshSession)>,
) {
    let is_edit = edit.is_some();
    let edit_id = edit.as_ref().map(|(id, _)| *id);
    let title: &'static str = if is_edit {
        "Edit SSH Session"
    } else {
        "New SSH Session"
    };

    // Prefill values (empty when creating new).
    let (
        label_val,
        host_val,
        port_val,
        user_val,
        group_val,
        color_val,
        auth_method,
        key_path,
        logging_val,
    ) = match &edit {
        Some((_, s)) => (
            s.label.clone(),
            s.host.clone(),
            s.port.to_string(),
            s.username.clone().unwrap_or_default(),
            s.group.clone().unwrap_or_default(),
            s.color.clone(),
            s.auth_method,
            s.key_path.clone(),
            s.logging,
        ),
        None => (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            None,
            SshAuthPreference::Password,
            None,
            SshLoggingOverride::Inherit,
        ),
    };

    // ── Collect existing groups from the store ──────────────────────────
    let existing_groups: Vec<SharedString> = {
        let store = SshSessionStore::global(cx);
        existing_group_names(store.read(cx).sessions())
    };

    // ── Shared cells for the Group Combobox ────────────────────────────
    let group_value: SharedCell = Rc::new(std::cell::RefCell::new(group_val.clone()));
    let query_cell: SharedCell = Rc::new(std::cell::RefCell::new(String::new()));

    // Find the selected index if group_val matches an existing group.
    let selected_indices: Vec<IndexPath> = existing_groups
        .iter()
        .position(|g| g.as_ref() == group_val)
        .map(|i| vec![IndexPath::default().row(i)])
        .unwrap_or_default();

    // ── Create InputState for the text fields ──────────────────────────
    let label_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("e.g. Production Server");
        if !label_val.is_empty() {
            st.set_value(label_val, window, cx);
        }
        st
    });
    let host_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("e.g. 192.168.1.10");
        if !host_val.is_empty() {
            st.set_value(host_val, window, cx);
        }
        st
    });
    let port_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("22");
        if !port_val.is_empty() {
            st.set_value(port_val, window, cx);
        }
        st
    });
    let user_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("optional");
        if !user_val.is_empty() {
            st.set_value(user_val, window, cx);
        }
        st
    });
    let auth_form = SshAuthForm::new(auth_method, key_path.as_deref(), window, cx);
    let logging = Rc::new(Cell::new(logging_val));

    // ── ColorPickerState ────────────────────────────────────────
    // Default colour tag when creating new, keep the old color when editing.
    let default_color_hex = color_val
        .clone()
        .unwrap_or_else(|| SshSession::DEFAULT_COLOR_HEX.to_string());
    let default_color = Hsla::parse_hex(&default_color_hex).unwrap_or(cx.theme().accent);
    let color_state = cx.new(|cx| {
        let mut st = ColorPickerState::new(window, cx);
        st.set_value(default_color, window, cx);
        st
    });

    // ── Create ComboboxState for the Group field ──────────────────────────
    let group_combo_state = cx.new(|cx| {
        let delegate = GroupComboDelegate::new(
            existing_groups.clone(),
            query_cell.clone(),
            group_value.clone(),
        );
        ComboboxState::new(delegate, selected_indices, window, cx).searchable(true)
    });

    // ── Shared save logic (confirm button + keyboard Enter) ──
    let submit = {
        let label_state = label_state.clone();
        let host_state = host_state.clone();
        let port_state = port_state.clone();
        let user_state = user_state.clone();
        let group_value = group_value.clone();
        let color_state = color_state.clone();
        let auth_form = auth_form.clone();
        let logging = logging.clone();
        move |window: &mut Window, cx: &mut App| {
            let form = SessionForm {
                label: label_state.read(cx).value().to_string(),
                host: host_state.read(cx).value().to_string(),
                port: port_state.read(cx).value().to_string(),
                username: user_state.read(cx).value().to_string(),
                group: group_value.borrow().clone(),
                color: color_state.read(cx).value().map(|h| h.to_hex()),
                auth_method: auth_form.method(),
                key_path: auth_form.key_path_value(cx),
                logging: logging.get(),
            };
            let session = match form.into_session() {
                Ok(session) => session,
                Err(message) => {
                    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
                    return false;
                }
            };
            let store = SshSessionStore::global(cx);
            match edit_id {
                Some(id) => store.update(cx, |s, cx| s.update(id, session, cx)),
                None => {
                    store.update(cx, |s, cx| s.add(session, cx));
                }
            }
            window.push_notification(
                notify(
                    NotificationType::Success,
                    if is_edit {
                        "SSH session updated."
                    } else {
                        "SSH session saved."
                    },
                    cx,
                ),
                cx,
            );
            true
        }
    };

    FormDialog::new(
        title,
        move |content, _window, cx| {
            content
                .child(labelled_field(
                    "Label",
                    FieldRequirement::Required,
                    h_flex()
                        .gap_2()
                        .w_full()
                        .child(Input::new(&label_state).flex_1())
                        .child(ColorPicker::new(&color_state).small()),
                    cx,
                ))
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
                    FieldRequirement::Optional,
                    Input::new(&user_state),
                    cx,
                ))
                .child(auth_form.render(false, cx))
                .child(labelled_field(
                    "Group",
                    FieldRequirement::Optional,
                    group_combobox(&group_combo_state, &group_value, &query_cell, cx),
                    cx,
                ))
                .child(labelled_field(
                    "Logging",
                    FieldRequirement::Optional,
                    h_flex().gap_4().children([
                        logging_radio(
                            "ssh-logging-inherit",
                            "Use global",
                            SshLoggingOverride::Inherit,
                            logging.clone(),
                        ),
                        logging_radio(
                            "ssh-logging-on",
                            "On",
                            SshLoggingOverride::On,
                            logging.clone(),
                        ),
                        logging_radio(
                            "ssh-logging-off",
                            "Off",
                            SshLoggingOverride::Off,
                            logging.clone(),
                        ),
                    ]),
                    cx,
                ))
        },
        submit,
    )
    .open(window, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled_form() -> SessionForm {
        SessionForm {
            label: "  prod  ".into(),
            host: " 10.0.0.1 ".into(),
            port: String::new(),
            username: "  ".into(),
            group: " ops ".into(),
            color: Some("#56B6C2".into()),
            auth_method: SshAuthPreference::Password,
            key_path: None,
            logging: SshLoggingOverride::Inherit,
        }
    }

    #[test]
    fn form_trims_fields_and_defaults_port_and_optional_values() {
        let session = filled_form().into_session().unwrap();
        assert_eq!(session.label, "prod");
        assert_eq!(session.host, "10.0.0.1");
        assert_eq!(session.port, SshSession::DEFAULT_PORT);
        assert_eq!(session.username, None);
        assert_eq!(session.group.as_deref(), Some("ops"));
        assert_eq!(session.color.as_deref(), Some("#56B6C2"));
        assert_eq!(session.key_path, None);
        assert_eq!(session.logging, SshLoggingOverride::Inherit);
    }

    #[test]
    fn form_preserves_ssh_logging_override() {
        let mut form = filled_form();
        form.logging = SshLoggingOverride::Off;
        assert_eq!(
            form.into_session().unwrap().logging,
            SshLoggingOverride::Off
        );
    }

    #[test]
    fn form_requires_label_and_host_and_a_valid_port() {
        let mut blank_label = filled_form();
        blank_label.label = "   ".into();
        assert_eq!(
            blank_label.into_session().unwrap_err(),
            "Label and Host are required."
        );
        let mut blank_host = filled_form();
        blank_host.host = String::new();
        assert!(blank_host.into_session().is_err());

        let mut bad_port = filled_form();
        bad_port.port = "70000".into();
        assert!(bad_port.into_session().unwrap_err().contains("70000"));
        let mut zero_port = filled_form();
        zero_port.port = "0".into();
        assert!(zero_port.into_session().is_err());
        let mut good_port = filled_form();
        good_port.port = " 2222 ".into();
        assert_eq!(good_port.into_session().unwrap().port, 2222);
    }

    #[test]
    fn form_requires_a_key_path_only_for_private_key_auth() {
        let mut key_auth = filled_form();
        key_auth.auth_method = SshAuthPreference::PrivateKey;
        assert_eq!(
            key_auth.clone().into_session().unwrap_err(),
            "Private key path is required."
        );
        key_auth.key_path = Some(PathBuf::from("/keys/id_ed25519"));
        let session = key_auth.into_session().unwrap();
        assert_eq!(session.auth_method, SshAuthPreference::PrivateKey);
        assert_eq!(
            session.key_path.as_deref(),
            Some(std::path::Path::new("/keys/id_ed25519"))
        );

        // A stale key path is dropped when password auth is selected.
        let mut password_auth = filled_form();
        password_auth.key_path = Some(PathBuf::from("/keys/id_ed25519"));
        assert_eq!(password_auth.into_session().unwrap().key_path, None);
    }

    #[test]
    fn existing_group_names_are_trimmed_sorted_and_unique() {
        let entry = |id: u64, group: Option<&str>| SshSessionEntry {
            id: SshSessionId::parse(&id.to_string()).unwrap(),
            session: SshSession {
                label: format!("s{id}"),
                host: "h".into(),
                port: 22,
                username: None,
                auth_method: SshAuthPreference::Password,
                key_path: None,
                color: None,
                group: group.map(str::to_string),
                logging: SshLoggingOverride::Inherit,
            },
        };
        let sessions = [
            entry(1, Some(" ops ")),
            entry(2, None),
            entry(3, Some("dev")),
            entry(4, Some("ops")),
            entry(5, Some("   ")),
        ];
        assert_eq!(
            existing_group_names(&sessions),
            vec![SharedString::from("dev"), SharedString::from("ops")]
        );
        assert!(existing_group_names(&[]).is_empty());
    }
}
