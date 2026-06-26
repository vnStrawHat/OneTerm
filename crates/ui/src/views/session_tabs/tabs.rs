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
//! - "New Session" / "Property" → mở dialog (xem [`super::session_dialog`]).
//! - "Open" / double-click → mở dialog connect (xem [`super::connect_dialog`]).

use std::collections::BTreeMap;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable as _, WindowExt as _,
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
use super::session_dialog::open_session_dialog;

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
            cx.notify();
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            store,
            tree_state,
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
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .text_sm()
            .child("No SSH session yet. Right-click → New Session.");

        // Tree widget.
        let tree_widget = tree(
            &tree_state,
            {
                let store = store.clone();
                move |ix, entry, _selected, _window, cx| {
                    let item = entry.item();
                    let depth = entry.depth();

                    if entry.is_folder() {
                        // Group folder.
                        let icon = if entry.is_expanded() {
                            IconName::FolderOpen
                        } else {
                            IconName::Folder
                        };
                        ListItem::new(ix)
                            .w_full()
                            .py_0()
                            .pl(px(16.) * depth as f32 + px(12.))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(Icon::new(icon).small().text_color(cx.theme().foreground))
                                    .child(item.label.clone()),
                            )
                    } else {
                        // Session leaf.
                        let store_ix = parse_session_id(&item.id);
                        let subtitle = store_ix
                            .and_then(|i| store.read(cx).sessions().get(i))
                            .map(|s| session_subtitle(s))
                            .unwrap_or_default();

                        ListItem::new(ix)
                            .w_full()
                            .py_0()
                            .pl(px(16.) * depth as f32 + px(12.))
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .justify_between()
                                    .gap_1()
                                    // Label — căn trái, truncate nếu dài.
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(cx.theme().foreground)
                                            .truncate()
                                            .child(item.label.clone()),
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
                    }
                }
            },
        )
        .context_menu({
            let focus = focus.clone();
            move |_ix, entry, menu, _window, _cx| {
                // Chỉ leaf (session) có context menu.
                if entry.is_folder() {
                    return menu;
                }
                let Some(store_ix) = parse_session_id(&entry.item().id) else {
                    return menu;
                };
                let focus = focus.clone();

                menu.action_context(focus)
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
                    // Right-click vào khu vực panel (không phải item) → New Session.
                    .context_menu({
                        let focus = focus.clone();
                        move |menu, _window, _cx| {
                            menu.action_context(focus.clone())
                                .menu("New Session", Box::new(NewSession))
                        }
                    }),
            )
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Parse store index từ TreeItem id (`session:{ix}`).
fn parse_session_id(id: &SharedString) -> Option<usize> {
    id.strip_prefix(SESSION_ID_PREFIX)
        .and_then(|s| s.parse::<usize>().ok())
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