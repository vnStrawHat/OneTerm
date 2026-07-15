//! Tree widget rendering — item renderer + context menu for SessionPanel.
//!
//! Split out from `tabs.rs` to keep the file shorter.

use gpui::prelude::FluentBuilder as _;
use gpui::{Hsla, IntoElement, MouseButton, ParentElement as _, SharedString, Styled, div, px};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, WindowExt as _, h_flex,
    list::ListItem, menu::PopupMenuItem, notification::NotificationType, tree::tree,
};

use crate::session_state::SshSessionStore;
use oneterm_actions::{DeleteSession, NewSession, OpenSession, SessionProperty};
use oneterm_state::notif_ext::notify;

use super::connect_dialog::open_connect_dialog;
use super::panel::SessionPanel;
use super::rename_group::open_rename_group_dialog;
use super::session_dialog::open_session_dialog;
use super::tree_builder::{parse_group_id, parse_session_id, session_subtitle};

impl SessionPanel {
    /// Render the tree widget — item renderer + context menu.
    ///
    /// Contains 2 large closures:
    /// 1. Item renderer — renders a folder (group) or leaf (session) with
    ///    icon, label, subtitle, and mouse handlers.
    /// 2. Context menu — right-click on an item → the appropriate menu (Open/Delete/Property).
    pub(crate) fn render_tree_widget(&self) -> impl IntoElement {
        let store = self.store.clone();
        let right_clicked_ix = self.right_clicked_ix.clone();
        let tree_state = self.tree_state.clone();
        let focus = self.focus_handle.clone();

        tree(&tree_state, {
            let store = store.clone();
            let right_clicked_ix = right_clicked_ix.clone();
            let tree_state = tree_state.clone();
            move |ix, entry, _selected, _window, cx| {
                let item = entry.item();
                let depth = entry.depth();
                let is_right_clicked = right_clicked_ix.get() == Some(ix);
                let hover_bg = cx.theme().tokens.list_hover;

                if entry.is_folder() {
                    // Group folder.
                    let (icon, icon_color) = if entry.is_expanded() {
                        (IconName::Maximize, gpui::rgb(0x58c4dc))
                    } else {
                        (IconName::Minimize, gpui::rgb(0x7c8a15))
                    };
                    ListItem::new(ix)
                        .w_full()
                        .py_0()
                        .pl(px(16.) * depth as f32 + px(12.))
                        .when(is_right_clicked, |this| this.bg(hover_bg))
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(Icon::new(icon).small().text_color(icon_color))
                                .child(item.label.clone()),
                        )
                        // Any click (left/right/middle) → highlight only this item.
                        // Right/Middle also set selected_index to clear the previous selection.
                        .on_mouse_down(MouseButton::Left, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            move |_, _, _| right_clicked_ix.set(Some(ix))
                        })
                        .on_mouse_down(MouseButton::Right, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                tree_state.update(cx, |s, cx| s.set_selected_index(Some(ix), cx));
                            }
                        })
                        .on_mouse_down(MouseButton::Middle, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                tree_state.update(cx, |s, cx| s.set_selected_index(Some(ix), cx));
                            }
                        })
                } else {
                    // Session leaf.
                    let store_ix = parse_session_id(&item.id);
                    let session = store_ix.and_then(|i| store.read(cx).sessions().get(i));
                    let subtitle = session.map(|s| session_subtitle(s)).unwrap_or_default();
                    let color = session
                        .and_then(|s| s.color.as_deref())
                        .and_then(|hex| Hsla::parse_hex(hex).ok())
                        .unwrap_or_else(|| Hsla::parse_hex("#56B6C2").unwrap_or(cx.theme().accent));

                    ListItem::new(ix)
                        .w_full()
                        .py_0()
                        .pl(px(16.) * depth as f32 + px(12.))
                        .when(is_right_clicked, |this| this.bg(hover_bg))
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .items_center()
                                .justify_between()
                                .gap_1()
                                // Colored square + Label — left aligned.
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .min_w_0()
                                        // Colored square.
                                        .child(div().w(px(8.)).h(px(8.)).bg(color).flex_shrink_0())
                                        // Label — truncate if long.
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(cx.theme().foreground)
                                                .truncate()
                                                .child(item.label.clone()),
                                        ),
                                )
                                // user@host:port — right aligned, muted.
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .flex_shrink_0()
                                        .child(SharedString::from(subtitle)),
                                ),
                        )
                        // Double-click → open the SSH connect dialog.
                        .on_click({
                            let store = store.clone();
                            let id = item.id.clone();
                            move |event, window, cx| {
                                if event.click_count() == 2 {
                                    if let Some(store_ix) = parse_session_id(&id) {
                                        if let Some(s) =
                                            store.read(cx).sessions().get(store_ix).cloned()
                                        {
                                            open_connect_dialog(s, store_ix, window, cx);
                                        }
                                    }
                                }
                            }
                        })
                        // Any click (left/right/middle) → highlight only this item.
                        // Right/Middle also set selected_index to clear the previous selection.
                        .on_mouse_down(MouseButton::Left, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            move |_, _, _| right_clicked_ix.set(Some(ix))
                        })
                        .on_mouse_down(MouseButton::Right, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                tree_state.update(cx, |s, cx| s.set_selected_index(Some(ix), cx));
                            }
                        })
                        .on_mouse_down(MouseButton::Middle, {
                            let right_clicked_ix = right_clicked_ix.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                tree_state.update(cx, |s, cx| s.set_selected_index(Some(ix), cx));
                            }
                        })
                }
            }
        })
        .context_menu({
            let focus = focus.clone();
            let right_clicked_ix = right_clicked_ix.clone();
            move |ix, entry, menu, _window, _cx| {
                // Clear the old highlight, highlight only the right-clicked item.
                right_clicked_ix.set(Some(ix));
                if entry.is_folder() {
                    // Group folder → context menu: New Session, Property.
                    let group = parse_group_id(&entry.item().id);
                    let focus = focus.clone();
                    menu.action_context(focus)
                        .menu("New Session", Box::new(NewSession))
                        .separator()
                        .item(
                            PopupMenuItem::new("Property").on_click(move |_, window, cx| {
                                if let Some(group_name) = &group {
                                    open_rename_group_dialog(window, cx, group_name.clone());
                                }
                            }),
                        )
                } else {
                    // Session leaf → context menu: New Session, Open, Delete, Property.
                    let Some(store_ix) = parse_session_id(&entry.item().id) else {
                        return menu;
                    };
                    let focus = focus.clone();

                    menu.action_context(focus)
                        .menu("New Session", Box::new(NewSession))
                        .separator()
                        .item(
                            PopupMenuItem::new("Open")
                                .action(Box::new(OpenSession))
                                .on_click(move |_, window, cx| {
                                    if let Some(s) = SshSessionStore::global(cx)
                                        .read(cx)
                                        .sessions()
                                        .get(store_ix)
                                        .cloned()
                                    {
                                        open_connect_dialog(s, store_ix, window, cx);
                                    }
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Delete")
                                .action(Box::new(DeleteSession))
                                .on_click(move |_, window, cx| {
                                    SshSessionStore::global(cx).update(cx, |s, cx| {
                                        s.remove(store_ix, cx);
                                    });
                                    window.push_notification(
                                        notify(
                                            NotificationType::Success,
                                            "SSH session deleted.",
                                            cx,
                                        ),
                                        cx,
                                    );
                                }),
                        )
                        .separator()
                        .item(
                            PopupMenuItem::new("Property")
                                .action(Box::new(SessionProperty))
                                .on_click(move |_, window, cx| {
                                    if let Some(s) = SshSessionStore::global(cx)
                                        .read(cx)
                                        .sessions()
                                        .get(store_ix)
                                        .cloned()
                                    {
                                        open_session_dialog(window, cx, Some((store_ix, s)));
                                    }
                                }),
                        )
                }
            }
        })
    }
}
