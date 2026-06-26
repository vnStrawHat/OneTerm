//! Dialog "New / Edit SSH Session" — tạo mới hoặc chỉnh sửa SSH session.
//!
//! Dialog có footer chứa 2 button: **Cancel** ([`DialogClose`]) và **Save**
//! ([`DialogAction`]). Khi Save → validate (Label & Host bắt buộc) →
//! `store.add` (tạo mới) hoặc `store.update` (chỉnh sửa) → auto-save
//! `ssh_session.json`.
//!
//! Form fields: Label, Host, Port, Username (optional), Group (optional).
//!
//! Group field dùng [`Combobox`] với `searchable(true)` + footer "Create" —
//! user có thể **chọn group có sẵn** hoặc **gõ group mới**.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{App, AppContext, InteractiveElement as _, SharedString, Task, Window, div, px};
use gpui::{IntoElement, ParentElement as _, Styled};
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, IndexPath, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    combobox::{Combobox, ComboboxState},
    dialog::{DialogAction, DialogButtonProps, DialogClose, DialogFooter},
    h_flex,
    input::{Input, InputState},
    searchable_list::{SearchableListDelegate, SearchableListItem, SearchableVec},
    v_flex,
};

use crate::state::{SshSession, SshSessionStore};

// ── GroupComboDelegate ───────────────────────────────────────────────

/// Shared mutable cell cho query text và group value.
/// Dùng `Rc<RefCell<>>` để delegate (bên trong ComboboxState) và footer
/// button (bên ngoài) cùng truy cập.
type SharedCell = Rc<RefCell<String>>;

/// Delegate cho Group Combobox — wraps [`SearchableVec`] + tracks query.
struct GroupComboDelegate {
    inner: SearchableVec<SharedString>,
    /// Search query hiện tại (cập nhật trong `perform_search`).
    query: SharedCell,
    /// Group value cuối cùng (cập nhật trong `on_confirm` hoặc footer click).
    group_value: SharedCell,
}

impl GroupComboDelegate {
    fn new(items: Vec<SharedString>, query: SharedCell, group_value: SharedCell) -> Self {
        Self {
            inner: SearchableVec::new(items),
            query,
            group_value,
        }
    }
}

impl SearchableListDelegate for GroupComboDelegate {
    type Item = SharedString;

    fn items_count(&self, section: usize) -> usize {
        self.inner.items_count(section)
    }

    fn item(&self, ix: IndexPath) -> Option<&SharedString> {
        self.inner.item(ix)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        SharedString: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.inner.position(value)
    }

    fn perform_search(&mut self, query: &str, window: &mut Window, cx: &mut App) -> Task<()> {
        *self.query.borrow_mut() = query.to_string();
        self.inner.perform_search(query, window, cx)
    }

    fn on_confirm(&mut self, final_selection: &[(IndexPath, SharedString)]) {
        if let Some((_, item)) = final_selection.first() {
            *self.group_value.borrow_mut() = item.to_string();
        } else {
            *self.group_value.borrow_mut() = String::new();
        }
    }
}

// ── open_session_dialog ──────────────────────────────────────────────

/// Mở dialog tạo mới (khi `edit` = `None`) hoặc chỉnh sửa (khi `edit` =
/// `Some((index, session))`) SSH session.
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

    // Giá trị prefill (rỗng nếu tạo mới).
    let (label_val, host_val, port_val, user_val, group_val) = match &edit {
        Some((_, s)) => (
            s.label.clone(),
            s.host.clone(),
            s.port.to_string(),
            s.username.clone().unwrap_or_default(),
            s.group.clone().unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new(), String::new(), String::new()),
    };

    // ── Thu thập existing groups từ store ──────────────────────────
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

    // ── Shared cells cho Group Combobox ────────────────────────────
    let group_value: SharedCell = Rc::new(RefCell::new(group_val.clone()));
    let query_cell: SharedCell = Rc::new(RefCell::new(String::new()));

    // Tìm selected index nếu group_val khớp với existing group.
    let selected_indices: Vec<IndexPath> = existing_groups
        .iter()
        .position(|g| g.as_ref() == group_val)
        .map(|i| vec![IndexPath::default().row(i)])
        .unwrap_or_default();

    // ── Tạo InputState cho các field text ──────────────────────────
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

    // ── Tạo ComboboxState cho Group field ──────────────────────────
    let group_combo_state = cx.new(|cx| {
        let delegate = GroupComboDelegate::new(
            existing_groups.clone(),
            query_cell.clone(),
            group_value.clone(),
        );
        ComboboxState::new(delegate, selected_indices, window, cx).searchable(true)
    });

    // Clone cho on_ok closure (đọc value khi Save).
    let label_ok = label_state.clone();
    let host_ok = host_state.clone();
    let port_ok = port_state.clone();
    let user_ok = user_state.clone();
    let group_ok = group_value.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title(title)
            .w(px(440.))
            .content({
                let label_state = label_state.clone();
                let host_state = host_state.clone();
                let port_state = port_state.clone();
                let user_state = user_state.clone();
                let group_combo_state = group_combo_state.clone();
                let group_value = group_value.clone();
                let query_cell = query_cell.clone();
                move |content, _window, cx| {
                    content
                        .child(field("Label", true, Input::new(&label_state), cx))
                        .child(field("Host", true, Input::new(&host_state), cx))
                        .child(field("Port", false, Input::new(&port_state), cx))
                        .child(field("Username", false, Input::new(&user_state), cx))
                        .child(field(
                            "Group",
                            false,
                            group_combobox(
                                &group_combo_state,
                                &group_value,
                                &query_cell,
                                cx,
                            ),
                            cx,
                        ))
                }
            })
            // Footer: Cancel (đóng dialog) + Save (dispatch ConfirmDialog → on_ok).
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new().child(Button::new("cancel").label("Cancel").outline()),
                    )
                    .child(DialogAction::new().child(Button::new("save").label("Save").primary())),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok({
                        let label_ok = label_ok.clone();
                        let host_ok = host_ok.clone();
                        let port_ok = port_ok.clone();
                        let user_ok = user_ok.clone();
                        let group_ok = group_ok.clone();
                        move |_, window, cx| {
                            let label = label_ok.read(cx).value().trim().to_string();
                            let host = host_ok.read(cx).value().trim().to_string();
                            if label.is_empty() || host.is_empty() {
                                window.push_notification("Label và Host là bắt buộc.", cx);
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
                            let session = SshSession {
                                label,
                                host,
                                port,
                                username,
                                group,
                            };
                            let store = SshSessionStore::global(cx);
                            match edit_index {
                                Some(ix) => store.update(cx, |s, cx| s.update(ix, session, cx)),
                                None => store.update(cx, |s, cx| s.add(session, cx)),
                            }
                            window.push_notification(
                                if is_edit {
                                    "SSH session đã được cập nhật."
                                } else {
                                    "SSH session đã được lưu."
                                },
                                cx,
                            );
                            true
                        }
                    }),
            )
    });
}


// ── open_rename_group_dialog ─────────────────────────────────────────

/// Mở dialog đổi tên group.
///
/// Hiển thị 1 input field với group name hiện tại. Khi Save →
/// `store.rename_group(old, new)` — cập nhật tất cả session trong group.
/// Nếu new name rỗng → ungroup (set group = None).
pub(crate) fn open_rename_group_dialog(
    window: &mut Window,
    cx: &mut App,
    group_name: String,
) {
    let group_state = cx.new(|cx| {
        let mut st = InputState::new(window, cx).placeholder("Group name");
        st.set_value(group_name.clone(), window, cx);
        st
    });

    let group_ok = group_state.clone();
    let old_name = group_name.clone();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        dialog
            .title("Rename Group")
            .w(px(440.))
            .content({
                let group_state = group_state.clone();
                move |content, _window, cx| {
                    content.child(field("Group Name", true, Input::new(&group_state), cx))
                }
            })
            .footer(
                DialogFooter::new()
                    .child(
                        DialogClose::new()
                            .child(Button::new("cancel").label("Cancel").outline()),
                    )
                    .child(
                        DialogAction::new().child(Button::new("save").label("Save").primary()),
                    ),
            )
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok({
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
                    }),
            )
    });
}
// ── group_combobox ───────────────────────────────────────────────────

/// Render Group field as a searchable [`Combobox`] với:
/// - **Trigger**: hiển thị `group_value` (hoặc placeholder nếu rỗng) +
///   chevron-down + optional clear (×) button.
/// - **Footer**: nút "Create '<query>'" — khi click → set `group_value`
///   = query text (cho phép tạo group mới).
fn group_combobox(
    state: &gpui::Entity<ComboboxState<GroupComboDelegate>>,
    group_value: &SharedCell,
    query_cell: &SharedCell,
    cx: &App,
) -> impl IntoElement {
    let group_value = group_value.clone();
    let query_cell = query_cell.clone();
    let muted_fg = cx.theme().muted_foreground;

    Combobox::new(state)
        .placeholder("Select or type group...")
        .search_placeholder("Search or type group name...")
        .w_full()
        .render_trigger({
            let group_value = group_value.clone();
            move |ctx, _, cx| {
                let val = group_value.borrow().clone();
                let placeholder = ctx
                    .placeholder
                    .cloned()
                    .unwrap_or_default();

                h_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .overflow_hidden()
                            .truncate()
                            .when(val.is_empty(), |this| {
                                this.text_color(cx.theme().muted_foreground)
                                    .child(placeholder)
                            })
                            .when(!val.is_empty(), |this| {
                                this.child(SharedString::from(val))
                            }),
                    )
                    .when(!ctx.open, |this| {
                        // Clear (×) button — chỉ hiện khi dropdown đóng và có value.
                        this.when(!group_value.borrow().is_empty(), |this| {
                            let gv = group_value.clone();
                            this.child(
                                div()
                                    .id("clear-group")
                                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                        cx.stop_propagation();
                                        *gv.borrow_mut() = String::new();
                                    })
                                    .child(
                                        Icon::new(IconName::CircleX)
                                            .xsmall()
                                            .text_color(muted_fg),
                                    ),
                            )
                        })
                    })
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .into_any_element()
            }
        })
        .footer({
            let group_value = group_value.clone();
            let query_cell = query_cell.clone();
            move |_, cx| {
                let query = query_cell.borrow().trim().to_string();
                let label = if query.is_empty() {
                    "Type to create new group".to_string()
                } else {
                    format!("Create \"{}\"", query)
                };
                let enabled = !query.is_empty();

                Button::new("create-group")
                    .ghost()
                    .label(label)
                    .icon(Icon::new(IconName::Plus))
                    .text_color(cx.theme().foreground)
                    .w_full()
                    .justify_start()
                    .when(!enabled, |this| this.disabled(true))
                    .when(enabled, |this| {
                        let gv = group_value.clone();
                        let q = query.clone();
                        this.on_click(move |_, _, _cx| {
                            *gv.borrow_mut() = q.clone();
                        })
                    })
                    .into_any_element()
            }
        })
}

// ── field helper ─────────────────────────────────────────────────────

/// Render 1 field form: label (có dấu `*` nếu bắt buộc) + input element.
pub(crate) fn field(
    label: &'static str,
    required: bool,
    input: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    let danger = cx.theme().danger;
    v_flex()
        .gap_1()
        .w_full()
        .child(
            h_flex()
                .gap_1()
                .text_sm()
                .child(SharedString::from(label))
                .when(required, |t| t.child(div().text_color(danger).child("*"))),
        )
        .child(input)
}