//! Tree widget rendering — item renderer + context menu for SessionPanel.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Hsla, IntoElement, MouseButton, ParentElement as _, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, h_flex,
    list::ListItem,
    menu::{PopupMenu, PopupMenuItem},
    tree::tree,
};

use crate::session_state::{SshSession, SshSessionId, SshSessionStore};
use oneterm_actions::{DeleteSession, NewSession, OpenSession, SessionProperty};

use super::connect_dialog::open_connect_dialog;
use super::panel::{SessionPanel, confirm_delete_session};
use super::rename_group::open_rename_group_dialog;
use super::session_dialog::{existing_group_names, open_session_dialog};
use super::tree_builder::{parse_group_id, parse_session_id, session_color_hex, session_subtitle};

/// One row of the context menu for a session leaf, in the order they appear.
///
/// The menu itself is not queryable once built, so the rows and their order
/// live here as data and are covered by a test (`US-0119`). The destructive
/// styling of `Delete` is proved by the captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionMenuRow {
    /// Open the connect dialog for the right-clicked session.
    Open,
    /// Edit the right-clicked session.
    Properties,
    /// Copy the right-clicked **saved** session and open the copy's properties.
    Duplicate,
    /// Submenu: move the right-clicked session into another group.
    MoveToGroup,
    /// Create a session — global, so it does not sit in the first slot.
    NewSession,
    /// Delete the right-clicked session, after a confirmation.
    Delete,
    Separator,
}

/// The session leaf's context menu, top to bottom.
///
/// The four actions that act on the right-clicked session come first; the global
/// "New Session" no longer occupies the most-misclicked slot (`F24`); and Delete
/// is last, behind its own separator and styled destructive, which is the shape
/// the SFTP browser's delete already uses.
pub(crate) const SESSION_MENU_ROWS: [SessionMenuRow; 8] = [
    SessionMenuRow::Open,
    SessionMenuRow::Properties,
    SessionMenuRow::Duplicate,
    SessionMenuRow::MoveToGroup,
    SessionMenuRow::Separator,
    SessionMenuRow::NewSession,
    SessionMenuRow::Separator,
    SessionMenuRow::Delete,
];

/// The Duplicate row's label.
///
/// Not just "Duplicate": a terminal tab's own context menu already has a row by
/// that name (`US-0116`) and it does something else — it reopens the *running*
/// connection in a second tab through `oneterm_core::SshDuplicateConfig`. This
/// one writes a second row into `ssh_session.json` and connects nothing, so it
/// says what it copies (`US-0129`).
const DUPLICATE_ROW_LABEL: &str = "Duplicate Saved Session";

/// The "Move to Group" submenu's ungrouped row.
const NO_GROUP_ROW_LABEL: &str = "No group";

/// The "Move to Group" submenu's route to the Group field of the dialog.
const NEW_GROUP_ROW_LABEL: &str = "New group\u{2026}";

/// Build the "Move to Group" submenu for `session_id`: the groups already in the
/// store (the same list the session dialog's combobox offers), "No group", and
/// "New group…", with the session's current group checked.
///
/// It is built while the parent menu is built, so it reads the store at
/// right-click time and cannot list a stale group. It is attached with
/// [`PopupMenuItem::submenu`] rather than `PopupMenu::submenu` because the
/// tree's context-menu builder holds a `Context<TreeState>`, not a
/// `Context<PopupMenu>`; the kit supports exactly that and wires the parent link
/// on the parent's next render.
fn move_to_group_submenu(
    session_id: SshSessionId,
    window: &mut Window,
    cx: &mut gpui::App,
) -> gpui::Entity<PopupMenu> {
    let store = SshSessionStore::global(cx);
    let current = store
        .read(cx)
        .get(session_id)
        .and_then(|session| session.group.clone());
    let groups = existing_group_names(store.read(cx).sessions());

    PopupMenu::build(window, cx, move |menu, _window, _cx| {
        let mut menu = menu.item(
            PopupMenuItem::new(NO_GROUP_ROW_LABEL)
                .checked(current.is_none())
                .on_click(move |_, _, cx| {
                    SshSessionStore::global(cx)
                        .update(cx, |store, cx| store.set_group(session_id, None, cx));
                }),
        );
        for group in groups {
            let checked = current.as_deref() == Some(group.as_ref());
            let target = group.clone();
            menu = menu.item(PopupMenuItem::new(group).checked(checked).on_click(
                move |_, _, cx| {
                    SshSessionStore::global(cx).update(cx, |store, cx| {
                        store.set_group(session_id, Some(target.as_ref()), cx)
                    });
                },
            ));
        }
        menu.separator()
            .item(
                PopupMenuItem::new(NEW_GROUP_ROW_LABEL).on_click(move |_, window, cx| {
                    if let Some(session) = SshSessionStore::global(cx)
                        .read(cx)
                        .get(session_id)
                        .cloned()
                    {
                        open_session_dialog(window, cx, Some((session_id, session)), true);
                    }
                }),
            )
    })
}

impl SessionPanel {
    /// Render the tree widget — item renderer + context menu.
    ///
    /// Contains 2 large closures:
    /// 1. Item renderer — renders a folder (group) or leaf (session) with
    ///    icon, label, subtitle, and mouse handlers.
    /// 2. Context menu — right-click on an item → the appropriate menu
    ///    (Open / Properties / Duplicate Saved Session / Move to Group ▸ /
    ///    New Session / Delete).
    pub(crate) fn render_tree_widget(&self) -> impl IntoElement {
        let store = self.store.clone();
        let right_clicked_ix = self.right_clicked_ix.clone();
        let row_was_right_clicked = self.row_was_right_clicked.clone();
        let tree_state = self.tree_state.clone();
        let focus = self.focus_handle.clone();

        tree(&tree_state, {
            let store = store.clone();
            let right_clicked_ix = right_clicked_ix.clone();
            let row_was_right_clicked = row_was_right_clicked.clone();
            let tree_state = tree_state.clone();
            move |ix, entry, _selected, _window, cx| {
                let item = entry.item();
                let depth = entry.depth();
                let is_right_clicked = right_clicked_ix.get() == Some(ix);
                let hover_bg = cx.theme().tokens.list_hover;

                if entry.is_folder() {
                    // A tree disclosure reads as a chevron: down when the group
                    // is open, right when it is closed (`F23`). It used to be
                    // the diagonal maximise arrow, which the app also uses for
                    // "zoom panel" — one glyph for two unrelated meanings. The
                    // direction carries the state, so the tint stays muted.
                    let icon = if entry.is_expanded() {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    };
                    let icon_color = cx.theme().muted_foreground;
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
                            let row_was_right_clicked = row_was_right_clicked.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                // The blank-area menu must stay empty for this
                                // right-click; see `row_was_right_clicked`.
                                row_was_right_clicked.set(true);
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
                    let session_id = parse_session_id(&item.id);
                    let session = session_id.and_then(|id| store.read(cx).get(id));
                    let subtitle = session.map(|s| session_subtitle(s)).unwrap_or_default();
                    // The same resolver the "+" menu's rows go through, so the
                    // two surfaces cannot disagree about a session's square
                    // (`US-0110`). A leaf whose id no longer resolves in the
                    // store takes the default too, as it did before the
                    // resolver existed; the accent is unreachable unless
                    // `DEFAULT_COLOR_HEX` itself stops parsing.
                    let color = Hsla::parse_hex(
                        session
                            .map(session_color_hex)
                            .unwrap_or(SshSession::DEFAULT_COLOR_HEX),
                    )
                    .unwrap_or_else(|_| cx.theme().accent);

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
                                    if let Some(session_id) = parse_session_id(&id) {
                                        if let Some(s) = store.read(cx).get(session_id).cloned() {
                                            open_connect_dialog(s, session_id, window, cx);
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
                            let row_was_right_clicked = row_was_right_clicked.clone();
                            let tree_state = tree_state.clone();
                            move |_, _, cx| {
                                right_clicked_ix.set(Some(ix));
                                // The blank-area menu must stay empty for this
                                // right-click; see `row_was_right_clicked`.
                                row_was_right_clicked.set(true);
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
            move |ix, entry, menu, window, cx| {
                // Clear the old highlight, highlight only the right-clicked item.
                right_clicked_ix.set(Some(ix));
                if entry.is_folder() {
                    // Group folder → rename the group, then the global action.
                    let group = parse_group_id(&entry.item().id);
                    let focus = focus.clone();
                    menu.action_context(focus)
                        .item(
                            PopupMenuItem::new("Rename Group…").on_click(move |_, window, cx| {
                                if let Some(group_name) = &group {
                                    open_rename_group_dialog(window, cx, group_name.clone());
                                }
                            }),
                        )
                        .separator()
                        .menu("New Session", Box::new(NewSession))
                } else {
                    // Session leaf → the rows of `SESSION_MENU_ROWS`, in order.
                    let Some(session_id) = parse_session_id(&entry.item().id) else {
                        return menu;
                    };
                    let mut menu = menu.action_context(focus.clone());
                    for row in SESSION_MENU_ROWS {
                        menu = match row {
                            SessionMenuRow::Separator => menu.separator(),
                            SessionMenuRow::NewSession => {
                                menu.menu("New Session", Box::new(NewSession))
                            }
                            SessionMenuRow::Open => menu.item(
                                PopupMenuItem::new("Open")
                                    .action(Box::new(OpenSession))
                                    .on_click(move |_, window, cx| {
                                        if let Some(s) = SshSessionStore::global(cx)
                                            .read(cx)
                                            .get(session_id)
                                            .cloned()
                                        {
                                            open_connect_dialog(s, session_id, window, cx);
                                        }
                                    }),
                            ),
                            SessionMenuRow::Properties => menu.item(
                                PopupMenuItem::new("Properties")
                                    .action(Box::new(SessionProperty))
                                    .on_click(move |_, window, cx| {
                                        if let Some(s) = SshSessionStore::global(cx)
                                            .read(cx)
                                            .get(session_id)
                                            .cloned()
                                        {
                                            open_session_dialog(
                                                window,
                                                cx,
                                                Some((session_id, s)),
                                                false,
                                            );
                                        }
                                    }),
                            ),
                            // A **saved** session's copy, not the live tab's
                            // Duplicate — see `DUPLICATE_ROW_LABEL`. It opens
                            // the copy's properties so the user can rename it;
                            // nothing connects.
                            SessionMenuRow::Duplicate => {
                                menu.item(PopupMenuItem::new(DUPLICATE_ROW_LABEL).on_click(
                                    move |_, window, cx| {
                                        let store = SshSessionStore::global(cx);
                                        let Some(copy_id) =
                                            store.update(cx, |s, cx| s.duplicate(session_id, cx))
                                        else {
                                            return;
                                        };
                                        if let Some(copy) = store.read(cx).get(copy_id).cloned() {
                                            open_session_dialog(
                                                window,
                                                cx,
                                                Some((copy_id, copy)),
                                                false,
                                            );
                                        }
                                    },
                                ))
                            }
                            SessionMenuRow::MoveToGroup => menu.item(PopupMenuItem::submenu(
                                "Move to Group",
                                move_to_group_submenu(session_id, window, cx),
                            )),
                            // `PopupMenuItem` has no destructive variant, so the
                            // row draws its own label in the theme's danger
                            // colour; the confirmation behind it is the SFTP
                            // browser's, reused (`US-0119`).
                            SessionMenuRow::Delete => menu.item(
                                PopupMenuItem::element(|_, cx| {
                                    div().text_color(cx.theme().danger).child("Delete")
                                })
                                .action(Box::new(DeleteSession))
                                .on_click(move |_, window, cx| {
                                    let store = SshSessionStore::global(cx);
                                    confirm_delete_session(store, session_id, window, cx);
                                }),
                            ),
                        };
                    }
                    menu
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `F24`: the global "New Session" no longer sits in the top slot of an
    /// item-specific menu, the two rows that act on the right-clicked session
    /// come first, and Delete is last.
    #[test]
    fn the_session_menu_leads_with_the_session_and_ends_with_delete() {
        let rows: Vec<SessionMenuRow> = SESSION_MENU_ROWS
            .into_iter()
            .filter(|row| *row != SessionMenuRow::Separator)
            .collect();
        assert_eq!(
            rows,
            vec![
                SessionMenuRow::Open,
                SessionMenuRow::Properties,
                SessionMenuRow::Duplicate,
                SessionMenuRow::MoveToGroup,
                SessionMenuRow::NewSession,
                SessionMenuRow::Delete,
            ]
        );
    }

    /// `US-0129`: the two new rows act on the right-clicked session, so they
    /// join the block above the first separator rather than sitting beside the
    /// global "New Session" or below the Delete separator.
    #[test]
    fn duplicate_and_move_to_group_join_the_session_block() {
        let first_separator = SESSION_MENU_ROWS
            .iter()
            .position(|row| *row == SessionMenuRow::Separator)
            .expect("the menu has a separator");
        assert_eq!(
            &SESSION_MENU_ROWS[..first_separator],
            &[
                SessionMenuRow::Open,
                SessionMenuRow::Properties,
                SessionMenuRow::Duplicate,
                SessionMenuRow::MoveToGroup,
            ]
        );
        // The row says what it copies, so it cannot be read as the terminal
        // tab's "Duplicate" (`US-0116`), which reopens a live connection.
        assert_ne!(DUPLICATE_ROW_LABEL, "Duplicate");
        assert!(DUPLICATE_ROW_LABEL.contains("Saved Session"));
    }

    /// Delete is separated from the rows above it, and it is the only
    /// destructive row.
    #[test]
    fn delete_is_destructive_and_stands_behind_a_separator() {
        let last = SESSION_MENU_ROWS.len() - 1;
        assert_eq!(SESSION_MENU_ROWS[last], SessionMenuRow::Delete);
        assert_eq!(SESSION_MENU_ROWS[last - 1], SessionMenuRow::Separator);
    }
}
