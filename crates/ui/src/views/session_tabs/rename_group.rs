//! "Rename Group" dialog — rename the group for all sessions in that group.
//!
//! Split out from `session_dialog.rs` to keep the file shorter.

use std::rc::Rc;

use gpui::{App, AppContext, ClickEvent, ParentElement as _, Window, px};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
    notification::NotificationType,
};

use crate::state::SshSessionStore;

use super::session_dialog::field;

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

    let group_ok = group_state.clone();
    let old_name = group_name.clone();

    // ── Shared save logic (used by both the button on_click and keyboard on_ok) ──
    let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let group_ok = group_ok.clone();
        let old_name = old_name.clone();
        move |_, window, cx| {
            let new_name = group_ok.read(cx).value().trim().to_string();
            if new_name.is_empty() {
                window.push_notification((NotificationType::Warning, "Group name cannot be empty."), cx);
                return false;
            }
            SshSessionStore::global(cx).update(cx, |s, cx| {
                s.rename_group(&old_name, &new_name, cx);
            });
            window.push_notification((NotificationType::Success, "Group renamed."), cx);
            true
        }
    });

    window.open_dialog(cx, move |dialog, _window, _cx| {
        // Clone save_logic for the button on_click and keyboard on_ok.
        let save_for_click = save_logic.clone();
        let save_for_kb = save_logic.clone();
        dialog
            .title("Rename Group")
            .w(px(440.))
            .content({
                let group_state = group_state.clone();
                move |content, _window, cx| {
                    content.child(field("Group Name", true, Input::new(&group_state), cx))
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
