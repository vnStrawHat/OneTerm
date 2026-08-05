//! "New / Edit SSH Session" dialog — create or edit an SSH session.
//!
//! Footer: **Cancel** + **Save** — uses direct on_click to bypass
//! action dispatch through the focus chain (consistent with the Connect dialog).
//! On Save → validate (Label & Host required) →
//! `store.add` (create) or `store.update` (edit) → auto-saves
//! `ssh_session.json`.
//!
//! Form fields: Label, Host, Port, Username (optional), Group (optional).
//!
//! The Group field uses a [`Combobox`] with `searchable(true)` + a "Create" footer —
//! the user can **pick an existing group** or **type a new one**.

use std::rc::Rc;

use gpui::{
    App, AppContext, ClickEvent, Hsla, ParentElement as _, SharedString, Styled, Window, px,
};
use gpui_component::{
    ActiveTheme, Colorize as _, IndexPath, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerState},
    combobox::ComboboxState,
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
};

use crate::session_state::{SshSession, SshSessionStore};
use oneterm_state::notif_ext::notify;

use super::common::{FieldRequirement, field};
use super::group_combo::{GroupComboDelegate, SharedCell, group_combobox};

/// Open the dialog to create (when `edit` = `None`) or edit (when `edit` =
/// `Some((index, session))`) an SSH session.
pub(crate) fn open_session_dialog(
    window: &mut Window,
    cx: &mut App,
    edit: Option<(usize, SshSession)>,
) {
    let is_edit = edit.is_some();
    let edit_index = edit.as_ref().map(|(ix, _)| *ix);
    let title: &'static str = if is_edit {
        "Edit SSH Session"
    } else {
        "New SSH Session"
    };

    // Prefill values (empty when creating new).
    let (label_val, host_val, port_val, user_val, group_val, color_val) = match &edit {
        Some((_, s)) => (
            s.label.clone(),
            s.host.clone(),
            s.port.to_string(),
            s.username.clone().unwrap_or_default(),
            s.group.clone().unwrap_or_default(),
            s.color.clone(),
        ),
        None => (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
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
            .filter_map(|s| s.group.as_ref().map(|g| g.trim().to_string()))
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

    // ── ColorPickerState ────────────────────────────────────────
    // Default: #56B6C2 when creating new, keep the old color when editing.
    let default_color_hex = color_val.clone().unwrap_or_else(|| "#56B6C2".to_string());
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

    // Clone for the on_ok closure (reads the value on Save).
    let label_ok = label_state.clone();
    let host_ok = host_state.clone();
    let port_ok = port_state.clone();
    let user_ok = user_state.clone();
    let group_ok = group_value.clone();
    let color_ok = color_state.clone();

    // ── Shared save logic (used by both the button on_click and keyboard on_ok) ──
    let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let label_ok = label_ok.clone();
        let host_ok = host_ok.clone();
        let port_ok = port_ok.clone();
        let user_ok = user_ok.clone();
        let group_ok = group_ok.clone();
        let color_ok = color_ok.clone();
        move |_, window, cx| {
            let label = label_ok.read(cx).value().trim().to_string();
            let host = host_ok.read(cx).value().trim().to_string();
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
            let port: u16 = port_ok
                .read(cx)
                .value()
                .trim()
                .parse()
                .unwrap_or(SshSession::DEFAULT_PORT);
            let username = {
                let u = user_ok.read(cx).value().trim().to_string();
                if u.is_empty() { None } else { Some(u) }
            };
            let group = {
                let g = group_ok.borrow().trim().to_string();
                if g.is_empty() { None } else { Some(g) }
            };
            let color = color_ok.read(cx).value().map(|h| h.to_hex());
            let session = SshSession {
                label,
                host,
                port,
                username,
                color,
                group,
            };
            let store = SshSessionStore::global(cx);
            match edit_index {
                Some(ix) => store.update(cx, |s, cx| s.update(ix, session, cx)),
                None => store.update(cx, |s, cx| s.add(session, cx)),
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
    });

    window.open_dialog(cx, move |dialog, _window, _cx| {
        // Clone save_logic for the button on_click and keyboard on_ok.
        let save_for_click = save_logic.clone();
        let save_for_kb = save_logic.clone();
        dialog
            .title(title)
            .w(px(440.))
            .content({
                let label_state = label_state.clone();
                let color_state = color_state.clone();
                let host_state = host_state.clone();
                let port_state = port_state.clone();
                let user_state = user_state.clone();
                let group_combo_state = group_combo_state.clone();
                let group_value = group_value.clone();
                let query_cell = query_cell.clone();
                move |content, _window, cx| {
                    content
                        .child(field(
                            "Label",
                            FieldRequirement::Required,
                            h_flex()
                                .gap_2()
                                .w_full()
                                .child(Input::new(&label_state).flex_1())
                                .child(ColorPicker::new(&color_state).small()),
                            cx,
                        ))
                        .child(field(
                            "Host",
                            FieldRequirement::Required,
                            Input::new(&host_state),
                            cx,
                        ))
                        .child(field(
                            "Port",
                            FieldRequirement::Optional,
                            Input::new(&port_state),
                            cx,
                        ))
                        .child(field(
                            "Username",
                            FieldRequirement::Optional,
                            Input::new(&user_state),
                            cx,
                        ))
                        .child(field(
                            "Group",
                            FieldRequirement::Optional,
                            group_combobox(&group_combo_state, &group_value, &query_cell, cx),
                            cx,
                        ))
                }
            })
            // Footer: Cancel + Save — uses direct on_click instead of DialogAction/DialogClose
            // to bypass action dispatch through the focus chain.
            .footer({
                DialogFooter::new()
                    .child(Button::new("cancel").label("Cancel").outline().on_click(
                        |_, window, cx| {
                            window.close_dialog(cx);
                        },
                    ))
                    .child(Button::new("save").label("Save").primary().on_click(
                        move |_, window, cx| {
                            if save_for_click(&ClickEvent::default(), window, cx) {
                                window.close_dialog(cx);
                            }
                        },
                    ))
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(move |_, window, cx| save_for_kb(&ClickEvent::default(), window, cx)),
            )
    });
}
