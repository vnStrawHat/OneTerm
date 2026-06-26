//! [`SessionPanel`] — leaf panel hiển thị danh sách SSH session dưới dạng Tree.
//!
//! Render Tree (1 level: Group → Item hoặc Item root) từ `ssh_session.json`
//! (qua [`crate::state::SshSessionStore`]) khi khởi động.
//!
//! - Item không có group → hiển thị ở root, trên cùng (sort theo label).
//! - Item có group → gom vào folder theo group name (sort theo group,
//!   trong group sort theo label).
//! - Double-click vào session item → mở dialog connect SSH.
//! - Right-click vào khu vực panel (trống) → context menu "New Session".
//! - Right-click vào 1 session item → context menu: Open, Delete, Property.
//! - Right-click vào 1 group folder → context menu: Property (rename group).
//! - "New Session" / "Property" → mở dialog (xem [`super::session_dialog`]).
//! - "Open" / double-click → mở dialog connect (xem [`super::connect_dialog`]).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Render, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Colorize as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    list::ListItem,
    menu::{ContextMenuExt, PopupMenuItem},
    tree::{TreeItem, TreeState, tree},
};

use crate::actions::NewSession;
use crate::state::{SshSession, SshSessionStore};

use super::connect_dialog::open_connect_dialog;
use super::session_dialog::{open_rename_group_dialog, open_session_dialog};

/// Prefix id cho leaf TreeItem (session) — encode store index.
const SESSION_ID_PREFIX: &str = "session:";
/// Prefix id cho folder TreeItem (group).
const GROUP_ID_PREFIX: &str = "group:";

/// Panel hiển thị danh sách SSH session dưới dạng Tree.
///
/// `panel_name = "session"`.
pub struct SessionPanel {
    focus_handle: FocusHandle,
    store: Entity<SshSessionStore>,
    tree_state: Entity<TreeState>,
    /// Track index bị click (bất kỳ button) để highlight — chỉ 1 item tại 1 thời điểm.
    right_clicked_ix: Rc<Cell<Option<usize>>>,
}

impl SessionPanel {
    /// Tạo panel mới — bind vào global [`SshSessionStore`] và observe để
    /// rebuild tree khi list session thay đổi.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        let tree_state = cx.new(|cx| TreeState::new(cx));

        // Build initial tree items.
        let items = build_tree_items(store.read(cx).sessions());
        tree_state.update(cx, |state, cx| state.set_items(items, cx));

        // Observe store → rebuild tree khi sessions thay đổi.
        cx.observe(&store, |this, store, cx| {
            let items = build_tree_items(store.read(cx).sessions());
            this.tree_state.update(cx, |state, cx| state.set_items(items, cx));
            this.right_clicked_ix.set(None);
            cx.notify();
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            store,
            tree_state,
            right_clicked_ix: Rc::new(Cell::new(None)),
        }
    }

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Action handler: mở dialog "New SSH Session" (tạo mới).
    pub(crate) fn on_new_session(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_session_dialog(window, cx, None);
    }
}

impl EventEmitter<PanelEvent> for SessionPanel {}

impl Focusable for SessionPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SessionPanel {
    fn panel_name(&self) -> &'static str {
        "session"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Session"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }
}

impl Render for SessionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let sessions = self.store.read(cx).sessions().to_vec();
        let focus = self.focus_handle.clone();
        let store = self.store.clone();
        let tree_state = self.tree_state.clone();
        let right_clicked_ix = self.right_clicked_ix.clone();

        // Header.
        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .text_color(theme.foreground)
                    .child(format!("Sessions ({})", sessions.len())),
            )
            .child(
                Button::new("new-session-btn")
                    .small()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("New SSH Session")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_new_session(&NewSession, window, cx);
                    })),
            );

        // Empty state.
        let empty = h_flex()
            .id("empty-state")
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .text_sm()
            .child("No SSH session yet. Right-click → New Session.")
            .context_menu({
                let focus = focus.clone();
                move |menu, _window, _cx| {
                    menu.action_context(focus.clone())
                        .menu("New Session", Box::new(NewSession))
                }
            });

        // Tree widget.
        let tree_widget = tree(
            &tree_state,
            {
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
                            // Right/Middle cũng set selected_index để clear selection cũ.
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
                                    // Colored square + Label — căn trái.
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .min_w_0()
                                            // Ô vuông màu.
                                            .child(
                                                div()
                                                    .w(px(8.))
                                                    .h(px(8.))
                                                    .bg(color)
                                                    .flex_shrink_0(),
                                            )
                                            // Label — truncate nếu dài.
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().foreground)
                                                    .truncate()
                                                    .child(item.label.clone()),
                                            ),
                                    )
                                    // user@host:port — căn phải, muted.
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .flex_shrink_0()
                                            .child(SharedString::from(subtitle)),
                                    ),
                            )
                            // Double-click → mở dialog connect SSH.
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
                            // Right/Middle cũng set selected_index để clear selection cũ.
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
            },
        )
        .context_menu({
            let focus = focus.clone();
            let right_clicked_ix = right_clicked_ix.clone();
            move |ix, entry, menu, _window, _cx| {
                // Clear highlight cũ, chỉ highlight item được right-click.
                right_clicked_ix.set(Some(ix));
                if entry.is_folder() {
                    // Group folder → context menu: New Session, Property.
                    let group = parse_group_id(&entry.item().id);
                    let focus = focus.clone();
                    menu.action_context(focus)
                        .menu("New Session", Box::new(NewSession))
                        .separator()
                        .item(PopupMenuItem::new("Property").on_click(move |_, window, cx| {
                            if let Some(group_name) = &group {
                                open_rename_group_dialog(window, cx, group_name.clone());
                            }
                        }))
                } else {
                    // Session leaf → context menu: New Session, Open, Delete, Property.
                    let Some(store_ix) = parse_session_id(&entry.item().id) else {
                        return menu;
                    };
                    let focus = focus.clone();

                    menu.action_context(focus)
                        .menu("New Session", Box::new(NewSession))
                        .separator()
                        .item(PopupMenuItem::new("Open").on_click(move |_, window, cx| {
                            if let Some(s) = SshSessionStore::global(cx)
                                .read(cx)
                                .sessions()
                                .get(store_ix)
                                .cloned()
                            {
                                open_connect_dialog(s, store_ix, window, cx);
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Delete").on_click(move |_, window, cx| {
                            SshSessionStore::global(cx).update(cx, |s, cx| {
                                s.remove(store_ix, cx);
                            });
                            window.push_notification("SSH session đã bị xoá.", cx);
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Property").on_click(move |_, window, cx| {
                            if let Some(s) = SshSessionStore::global(cx)
                                .read(cx)
                                .sessions()
                                .get(store_ix)
                                .cloned()
                            {
                                open_session_dialog(window, cx, Some((store_ix, s)));
                            }
                        }))
                }
            }
        });

        div()
            .id("session-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_session))
            .flex()
            .flex_col()
            .bg(theme.background)
            .child(header)
            .child(
                div()
                    .id("session-list")
                    .flex_1()
                    .min_h_0()
                    .when(sessions.is_empty(), |t| t.child(empty))
                    .when(!sessions.is_empty(), |t| t.child(tree_widget))
            )
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Parse store index từ TreeItem id (`session:{ix}`).
fn parse_session_id(id: &SharedString) -> Option<usize> {
    id.strip_prefix(SESSION_ID_PREFIX)
        .and_then(|s| s.parse::<usize>().ok())
}

/// Parse group name từ TreeItem id (`group:{name}`).
fn parse_group_id(id: &SharedString) -> Option<String> {
    id.strip_prefix(GROUP_ID_PREFIX).map(|s| s.to_string())
}

/// Tạo subtitle cho session leaf: `user@host:port` hoặc `host:port`.
fn session_subtitle(s: &SshSession) -> String {
    match &s.username {
        Some(u) => format!("{}@{}:{}", u, s.host, s.port),
        None => format!("{}:{}", s.host, s.port),
    }
}

/// Build `Vec<TreeItem>` từ danh sách session — áp dụng grouping + sorting.
///
/// - Item không có group → root (trên cùng), sort theo label.
/// - Item có group → folder theo group name (sort), trong folder sort theo label.
fn build_tree_items(sessions: &[SshSession]) -> Vec<TreeItem> {
    // 1. Tách ungrouped và grouped.
    let mut ungrouped: Vec<(usize, &SshSession)> = Vec::new();
    let mut groups: BTreeMap<String, Vec<(usize, &SshSession)>> = BTreeMap::new();

    for (ix, s) in sessions.iter().enumerate() {
        match &s.group {
            Some(g) if !g.trim().is_empty() => {
                groups
                    .entry(g.trim().to_string())
                    .or_default()
                    .push((ix, s));
            }
            _ => {
                ungrouped.push((ix, s));
            }
        }
    }

    // 2. Sort ungrouped theo label.
    ungrouped.sort_by(|a, b| a.1.label.to_lowercase().cmp(&b.1.label.to_lowercase()));

    // 3. Root items: ungrouped trước, rồi đến groups (BTreeMap đã sort theo key).
    let mut items = Vec::new();

    // Ungrouped sessions ở root.
    for (ix, s) in &ungrouped {
        items.push(TreeItem::new(
            format!("{SESSION_ID_PREFIX}{ix}"),
            s.label.clone(),
        ));
    }

    // Groups.
    for (group, mut group_sessions) in groups {
        group_sessions.sort_by(|a, b| {
            a.1.label
                .to_lowercase()
                .cmp(&b.1.label.to_lowercase())
        });
        let children = group_sessions
            .iter()
            .map(|(ix, s)| TreeItem::new(format!("{SESSION_ID_PREFIX}{ix}"), s.label.clone()))
            .collect::<Vec<_>>();
        items.push(
            TreeItem::new(format!("{GROUP_ID_PREFIX}{group}"), group)
                .expanded(true)
                .children(children),
        );
    }

    items
}
