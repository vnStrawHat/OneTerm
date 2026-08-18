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

use std::rc::Rc;

use gpui::{App, AppContext, Hsla, ParentElement as _, SharedString, Styled, Window};
use gpui_component::{
    ActiveTheme, Colorize as _, IndexPath, Sizable as _, WindowExt as _,
    color_picker::{ColorPicker, ColorPickerState},
    combobox::ComboboxState,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_state::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::parse_port;
use super::group_combo::{GroupComboDelegate, SharedCell, group_combobox};
use crate::session_state::{SshAuthPreference, SshSession, SshSessionId, SshSessionStore};

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
    let (label_val, host_val, port_val, user_val, group_val, color_val, auth_method, key_path) =
        match &edit {
            Some((_, s)) => (
                s.label.clone(),
                s.host.clone(),
                s.port.to_string(),
                s.username.clone().unwrap_or_default(),
                s.group.clone().unwrap_or_default(),
                s.color.clone(),
                s.auth_method,
                s.key_path.clone(),
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
            ),
        };

    // ── Collect existing groups from the store ──────────────────────────
    let existing_groups: Vec<SharedString> = {
        let store = SshSessionStore::global(cx);
        let store = store.read(cx);
        let mut groups: Vec<String> = store
            .sessions()
            .iter()
            .filter_map(|entry| entry.session.group.as_ref().map(|g| g.trim().to_string()))
            .filter(|g| !g.is_empty())
            .collect();
        groups.sort();
        groups.dedup();
        groups.into_iter().map(SharedString::from).collect()
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
        move |window: &mut Window, cx: &mut App| {
            let label = label_state.read(cx).value().trim().to_string();
            let host = host_state.read(cx).value().trim().to_string();
            if label.is_empty() || host.is_empty() {
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Label and Host are required.",
                        cx,
                    ),
                    cx,
                );
                return false;
            }
            let port_text = port_state.read(cx).value().trim().to_string();
            let port = if port_text.is_empty() {
                SshSession::DEFAULT_PORT
            } else {
                match parse_port(&port_text) {
                    Ok(port) => port,
                    Err(error) => {
                        window.push_notification(
                            notify(NotificationType::Warning, error.to_string(), cx),
                            cx,
                        );
                        return false;
                    }
                }
            };
            let username = {
                let u = user_state.read(cx).value().trim().to_string();
                if u.is_empty() { None } else { Some(u) }
            };
            let group = {
                let g = group_value.borrow().trim().to_string();
                if g.is_empty() { None } else { Some(g) }
            };
            let color = color_state.read(cx).value().map(|h| h.to_hex());
            let auth_method = auth_form.method();
            let key_path = if auth_method == SshAuthPreference::PrivateKey {
                match auth_form.key_path_value(cx) {
                    Some(path) => Some(path),
                    None => {
                        window.push_notification(
                            notify(
                                NotificationType::Warning,
                                "Private key path is required.",
                                cx,
                            ),
                            cx,
                        );
                        return false;
                    }
                }
            } else {
                None
            };
            let session = SshSession {
                label,
                host,
                port,
                username,
                auth_method,
                key_path,
                color,
                group,
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
                .child(auth_form.render_preference(cx))
                .child(labelled_field(
                    "Group",
                    FieldRequirement::Optional,
                    group_combobox(&group_combo_state, &group_value, &query_cell, cx),
                    cx,
                ))
        },
        submit,
    )
    .open(window, cx);
}
