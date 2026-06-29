//! Dialog "Rename Group" — đổi tên group cho tất cả session trong group đó.
//!
//! Tách từ `session_dialog.rs` để giảm độ dài file.

use std::rc::Rc;

use gpui::{App, AppContext, ClickEvent, ParentElement as _, Window, px};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    input::{Input, InputState},
};

use crate::state::SshSessionStore;

use super::session_dialog::field;

/// Mở dialog đổi tên group.
///
/// Hiển thị 1 input field với group name hiện tại. Khi Save →
/// `store.rename_group(old, new)` — cập nhật tất cả session trong group.
/// Nếu new name rỗng → ungroup (set group = None).
pub(crate) fn open_rename_group_dialog(window: &mut Window, cx: &mut App, group_name: String) {
    let group_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("Group name");
        st.set_value(group_name.clone(), window, cx);
        st
    });

    let group_ok = group_state.clone();
    let old_name = group_name.clone();

    // ── Shared save logic (dùng cho cả button on_click và keyboard on_ok) ──
    let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
        let group_ok = group_ok.clone();
        let old_name = old_name.clone();
        move |_, window, cx| {
            let new_name = group_ok.read(cx).value().trim().to_string();
            if new_name.is_empty() {
                window.push_notification("Group name không được rỗng.", cx);
                return false;
            }
            SshSessionStore::global(cx).update(cx, |s, cx| {
                s.rename_group(&old_name, &new_name, cx);
            });
            window.push_notification("Group đã được đổi tên.", cx);
            true
        }
    });

    window.open_dialog(cx, move |dialog, _window, _cx| {
        // Clone save_logic cho button on_click và keyboard on_ok
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
            // Footer: Cancel + Save — dùng direct on_click thay vì DialogAction/DialogClose
            // để bypass action dispatch qua focus chain.
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
