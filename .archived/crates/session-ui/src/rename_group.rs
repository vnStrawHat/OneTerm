//! "Rename Group" dialog — rename the group for all sessions in that group.

use gpui::{App, AppContext, ParentElement as _, Window};
use gpui_component::{
    WindowExt as _,
    input::{Input, InputState},
    notification::NotificationType,
};

use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_theme::notif_ext::notify;

use crate::session_state::SshSessionStore;

/// Open the rename-group dialog.
///
/// Shows one input field with the current group name. On Save →
/// `store.rename_group(old, new)` — updates all sessions in the group.
/// If the new name is empty → ungroup (set group = None).
pub(crate) fn open_rename_group_dialog(window: &mut Window, cx: &mut App, group_name: String) {
    let group_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("Group name");
        st.set_value(group_name.clone(), window, cx);
        st
    });

    let submit = {
        let group_state = group_state.clone();
        let old_name = group_name;
        move |window: &mut Window, cx: &mut App| {
            let new_name = group_state.read(cx).value().trim().to_string();
            if new_name.is_empty() {
                window.push_notification(
                    notify(NotificationType::Warning, "Group name cannot be empty.", cx),
                    cx,
                );
                return false;
            }
            SshSessionStore::global(cx).update(cx, |s, cx| {
                s.rename_group(&old_name, &new_name, cx);
            });
            window.push_notification(notify(NotificationType::Success, "Group renamed.", cx), cx);
            true
        }
    };

    FormDialog::new(
        "Rename Group",
        move |content, _window, cx| {
            content.child(labelled_field(
                "Group Name",
                FieldRequirement::Required,
                Input::new(&group_state),
                cx,
            ))
        },
        submit,
    )
    .open(window, cx);
}
