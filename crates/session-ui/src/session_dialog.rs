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

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Colorize as _, Icon, IconName, IndexPath, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    color_picker::{ColorPicker, ColorPickerState},
    combobox::ComboboxState,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    radio::Radio,
};

use oneterm_core::PortForward;
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, control_label, labelled_field};
use oneterm_theme::notif_ext::notify;

use super::auth_form::SshAuthForm;
use super::common::parse_port;
use super::forward_rows::PortForwardRows;
use super::group_combo::{GroupComboDelegate, MatchCount, SharedCell, group_combobox};
use super::jump_hops::JumpHostPicker;
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
    jump_host: Option<SshSessionId>,
    port_forwards: Vec<PortForward>,
    agent_forwarding: bool,
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
            SshAuthPreference::Password | SshAuthPreference::Agent => None,
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
            jump_host: self.jump_host,
            port_forwards: self.port_forwards,
            agent_forwarding: self.agent_forwarding,
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

/// Whether the session already uses one of the Advanced fields.
///
/// The disclosure starts open when it does: a user who set a jump host and then
/// sees no jump host concludes it was lost, which is worse than a long form
/// (`US-0120`).
fn advanced_is_configured(
    jump_host: Option<SshSessionId>,
    port_forwards: &[PortForward],
    agent_forwarding: bool,
) -> bool {
    jump_host.is_some() || !port_forwards.is_empty() || agent_forwarding
}

/// The "Advanced" disclosure header.
///
/// A kit [`Button`], not a styled `h_flex`: the disclosure is the **only** route
/// to jump host, agent forwarding and port forwards, and those three were plain
/// Tab stops before they were folded away. A `div` with an `on_click` is not
/// focusable, so a keyboard-only user could no longer reach them at all
/// (`US-0120` rework). A `Button` is a tab stop, announces itself as a button,
/// and toggles on **Space**. Not Enter: `FormDialog` binds Enter to submit in
/// the dialog's key context, and gpui dispatches a keymap binding before any
/// element's key listener, so Enter never reaches the button — the same as for
/// Browse, Cancel and Save in this dialog.
fn advanced_header(expanded: Rc<Cell<bool>>, cx: &App) -> impl IntoElement {
    let open = expanded.get();
    let icon = if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    Button::new("advanced-disclosure")
        .ghost()
        .small()
        .icon(Icon::new(icon).xsmall())
        .accessibility_label(if open {
            "Advanced, expanded"
        } else {
            "Advanced, collapsed"
        })
        .child(control_label("Advanced"))
        .text_color(cx.theme().muted_foreground)
        .on_click(move |_, window, _| {
            expanded.set(!expanded.get());
            window.refresh();
        })
}

/// Which part of the form a refused Save is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidField {
    /// A port-forward row — behind the Advanced disclosure.
    PortForward,
    /// The jump-host picker — behind the Advanced disclosure.
    JumpHost,
    /// Label, Host, Port, Username, the key path: always on screen.
    Basic,
}

/// Whether a refused Save must open the Advanced disclosure before it shows the
/// message.
///
/// `submit` validates the forwards and the jump chain whether or not they are on
/// screen, so a collapsed disclosure could leave the user blocked by a message
/// naming a field they cannot see (`US-0120` rework). Opening it first is the
/// whole fix; a basic field is already visible and must not make the form jump.
fn reveals_advanced(field: InvalidField) -> bool {
    matches!(field, InvalidField::PortForward | InvalidField::JumpHost)
}

/// The eight colours the short row offers, and the full picker's featured row.
///
/// **This is the one place the set is defined.** The kit exposes no accessor for
/// its own default featured row — only a setter, `ColorPicker::featured_colors`
/// — so the row is defined here and *given* to the picker, which is why the
/// eight swatches and the eight along the top of the picker are the same eight
/// (`US-0120` rework; before, the row was eight and the picker's own was a
/// different twelve).
///
/// The first entry is `US-0110`'s default so a new session's colour is one of
/// the eight; it is the one hard-coded value, and it is hard-coded in
/// `session_state.rs`, not here. The rest come from the theme.
fn swatch_colors(cx: &App) -> [Hsla; 8] {
    let theme = cx.theme();
    [
        Hsla::parse_hex(SshSession::DEFAULT_COLOR_HEX).unwrap_or(theme.cyan),
        theme.red,
        theme.yellow,
        theme.green,
        theme.blue,
        theme.magenta,
        theme.red_light,
        theme.blue_light,
    ]
}

/// The session colour row: eight swatches and a route to the full picker.
///
/// A swatch writes through the same `ColorPickerState` the full picker writes,
/// and `submit` still reads `state.value().to_hex()`, so both surfaces produce
/// exactly the value `session_color_hex` already accepted (`US-0110`). Choosing
/// a colour costs one click instead of a decision among 130 swatches, and the
/// control finally says what it is.
fn color_row(state: &gpui::Entity<ColorPickerState>, cx: &App) -> impl IntoElement {
    let selected = state.read(cx).value();
    let theme = cx.theme();
    let border = theme.primary;
    h_flex()
        .gap_2()
        .items_center()
        .flex_wrap()
        .children(
            swatch_colors(cx)
                .into_iter()
                .enumerate()
                .map(|(ix, color)| {
                    let is_selected =
                        selected.is_some_and(|value| value.to_hex() == color.to_hex());
                    let state = state.clone();
                    div()
                        .id(("session-color", ix))
                        .w_5()
                        .h_5()
                        .rounded_sm()
                        .bg(color)
                        .border_2()
                        .border_color(if is_selected {
                            border
                        } else {
                            gpui::transparent_black()
                        })
                        .role(Role::Button)
                        .aria_label(color.to_hex())
                        .on_click(move |_, window, cx| {
                            state.update(cx, |state, cx| state.set_value(color, window, cx));
                        })
                }),
        )
        .child(
            // "Custom…" belongs *inside* the picker's trigger, so clicking the
            // word opens the picker; it used to be a bare `div` beside it. The
            // icon replaces the trigger's current-value square, which made a
            // ninth square in a row of eight — indistinguishable from swatch 1
            // whenever the default colour was selected. `ColorPickerButton`
            // draws that square only when it has no icon. The picker's own
            // featured row is set from `swatch_colors`, so the short row and the
            // top of the popup are one list.
            ColorPicker::new(state)
                .small()
                .featured_colors(swatch_colors(cx).to_vec())
                .icon(Icon::new(IconName::Palette))
                .label("Custom\u{2026}")
                .accessibility_label("Custom colour\u{2026}"),
        )
}

fn logging_radio(
    id: &'static str,
    label: &'static str,
    value: SshLoggingOverride,
    selected: Rc<Cell<SshLoggingOverride>>,
) -> Radio {
    // `control_label` instead of `.label(label)`, and `items_center` so the
    // taller label box stays level with the indicator — see BUG-0069.
    Radio::new(id)
        .items_center()
        .accessibility_label(label)
        .child(control_label(label))
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
        jump_host_val,
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
            s.jump_host,
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
            None,
        ),
    };
    let jump_host_picker = JumpHostPicker::new(edit_id, jump_host_val, window, cx);
    let saved_forwards: Vec<PortForward> = edit
        .as_ref()
        .map(|(_, s)| s.port_forwards.clone())
        .unwrap_or_default();
    let forward_rows = PortForwardRows::new(&saved_forwards, window, cx);
    let agent_forwarding = Rc::new(Cell::new(
        edit.as_ref().is_some_and(|(_, s)| s.agent_forwarding),
    ));
    let advanced_expanded = Rc::new(Cell::new(advanced_is_configured(
        jump_host_val,
        &saved_forwards,
        agent_forwarding.get(),
    )));

    // ── Collect existing groups from the store ──────────────────────────
    let existing_groups: Vec<SharedString> = {
        let store = SshSessionStore::global(cx);
        existing_group_names(store.read(cx).sessions())
    };

    // Whether the store holds any group at all, so the dropdown's no-match
    // area can tell "none exist" from "none match what you typed".
    let has_any_group = !existing_groups.is_empty();

    // ── Shared cells for the Group Combobox ────────────────────────────
    let group_value: SharedCell = Rc::new(std::cell::RefCell::new(group_val.clone()));
    let query_cell: SharedCell = Rc::new(std::cell::RefCell::new(String::new()));
    // How many rows the dropdown's search left, so Enter can tell "nothing to
    // select, create it" from "the list has a match and Enter is its key".
    let match_count: MatchCount = Rc::new(Cell::new(0));

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
            match_count.clone(),
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
        let jump_host_picker = jump_host_picker.clone();
        let forward_rows = forward_rows.clone();
        let agent_forwarding = agent_forwarding.clone();
        let advanced_expanded = advanced_expanded.clone();
        move |window: &mut Window, cx: &mut App| {
            let store = SshSessionStore::global(cx);
            // A refused Save must never name a field the disclosure is hiding:
            // open it and put the cursor in the field first, then say why
            // (`US-0120` rework).
            let reveal = |field: InvalidField| {
                if reveals_advanced(field) {
                    advanced_expanded.set(true);
                }
            };
            let port_forwards = match forward_rows.take(cx) {
                Ok(forwards) => forwards,
                Err(error) => {
                    reveal(InvalidField::PortForward);
                    forward_rows.focus_row(error.row, window, cx);
                    window.push_notification(
                        notify(NotificationType::Warning, error.message, cx),
                        cx,
                    );
                    return false;
                }
            };
            let jump_host = jump_host_picker.selected(cx);
            if let Err(error) = store.read(cx).jump_chain(jump_host, edit_id) {
                reveal(InvalidField::JumpHost);
                jump_host_picker.focus(window, cx);
                window.push_notification(
                    notify(NotificationType::Warning, error.to_string(), cx),
                    cx,
                );
                return false;
            }
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
                jump_host,
                port_forwards,
                agent_forwarding: agent_forwarding.get(),
            };
            let session = match form.into_session() {
                Ok(session) => session,
                Err(message) => {
                    // A basic field is already on screen; the form must not jump.
                    reveal(InvalidField::Basic);
                    window.push_notification(notify(NotificationType::Warning, message, cx), cx);
                    return false;
                }
            };
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
                    Input::new(&label_state),
                    cx,
                ))
                .child(labelled_field(
                    "Color",
                    FieldRequirement::Optional,
                    color_row(&color_state, cx),
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
                // Jump host, agent forwarding and port forwards fold away: most
                // sessions use none of them, and the three of them are most of
                // the form's height (`US-0120`).
                .child(advanced_header(advanced_expanded.clone(), cx))
                .when(advanced_expanded.get(), |content| {
                    content
                        .child(jump_host_picker.render(cx))
                        .child(
                            Checkbox::new("agent-forwarding")
                                .items_center()
                                .accessibility_label("Forward the SSH agent to the remote host")
                                .child(control_label("Forward the SSH agent to the remote host"))
                                .checked(agent_forwarding.get())
                                .on_click({
                                    let agent_forwarding = agent_forwarding.clone();
                                    move |checked: &bool, _window, _cx| {
                                        agent_forwarding.set(*checked)
                                    }
                                }),
                        )
                        .child(forward_rows.render(cx))
                })
                .child(labelled_field(
                    "Group",
                    FieldRequirement::Optional,
                    group_combobox(
                        &group_combo_state,
                        &group_value,
                        &query_cell,
                        &match_count,
                        has_any_group,
                        cx,
                    ),
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
    // Wide enough for one port-forward row per line. The body scrolls when it
    // outgrows the window; `FormDialog` owns that now (`US-0120`).
    .width(px(560.))
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
            jump_host: None,
            port_forwards: Vec::new(),
            agent_forwarding: false,
        }
    }

    #[test]
    fn form_keeps_the_agent_forwarding_switch() {
        let mut form = filled_form();
        form.agent_forwarding = true;
        assert!(form.into_session().unwrap().agent_forwarding);
        assert!(!filled_form().into_session().unwrap().agent_forwarding);
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

        // A stale key path is dropped when password or agent auth is selected.
        let mut password_auth = filled_form();
        password_auth.key_path = Some(PathBuf::from("/keys/id_ed25519"));
        assert_eq!(password_auth.into_session().unwrap().key_path, None);
        let mut agent_auth = filled_form();
        agent_auth.auth_method = SshAuthPreference::Agent;
        agent_auth.key_path = Some(PathBuf::from("/keys/id_ed25519"));
        let session = agent_auth.into_session().unwrap();
        assert_eq!(session.auth_method, SshAuthPreference::Agent);
        assert_eq!(session.key_path, None);
    }

    /// `US-0120`: the disclosure is collapsed for a new session and open for a
    /// session that already uses any of the three fields behind it.
    #[test]
    fn advanced_opens_only_when_the_session_already_uses_it() {
        let id = SshSessionId::parse("7").unwrap();
        let forward = PortForward::Dynamic {
            bind: oneterm_core::loopback(),
            bind_port: 1080,
        };
        assert!(!advanced_is_configured(None, &[], false));
        assert!(advanced_is_configured(Some(id), &[], false));
        assert!(advanced_is_configured(None, &[forward], false));
        assert!(advanced_is_configured(None, &[], true));
    }

    /// `US-0120` rework: a refused Save opens the disclosure when, and only
    /// when, the field it names is behind it.
    #[test]
    fn a_refused_save_opens_the_disclosure_only_for_a_field_it_hides() {
        assert!(reveals_advanced(InvalidField::PortForward));
        assert!(reveals_advanced(InvalidField::JumpHost));
        // Label, Host, Port, Username, the key path: visible already, so the
        // form must not jump under the user's cursor.
        assert!(!reveals_advanced(InvalidField::Basic));
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
                jump_host: None,
                port_forwards: Vec::new(),
                agent_forwarding: false,
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
