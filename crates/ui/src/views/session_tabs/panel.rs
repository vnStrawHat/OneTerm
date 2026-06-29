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
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Task,
    Window,
};
use gpui_component::{
    dock::{Panel, PanelControl, PanelEvent},
    input::InputState,
    tree::TreeState,
};

use crate::actions::NewSession;
use crate::state::SshSessionStore;

use super::session_dialog::open_session_dialog;
use super::tree_builder::build_tree_items;

/// Prefix id cho leaf TreeItem (session) — encode store index.
pub(crate) const SESSION_ID_PREFIX: &str = "session:";
/// Prefix id cho folder TreeItem (group).
pub(crate) const GROUP_ID_PREFIX: &str = "group:";

/// Panel hiển thị danh sách SSH session dưới dạng Tree.
///
/// `panel_name = "session"`.
pub struct SessionPanel {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) store: Entity<SshSessionStore>,
    pub(crate) tree_state: Entity<TreeState>,
    /// Search input state — filter sessions theo label/host/user/group.
    pub(crate) search_state: Entity<InputState>,
    /// Debounce task cho search — thay task cũ = cancel task cũ (debounce).
    pub(crate) search_debounce_task: Option<Task<()>>,
    /// Track index bị click (bất kỳ button) để highlight — chỉ 1 item tại 1 thời điểm.
    pub(crate) right_clicked_ix: Rc<Cell<Option<usize>>>,
}

impl SessionPanel {
    /// Tạo panel mới — bind vào global [`SshSessionStore`] và observe để
    /// rebuild tree khi list session thay đổi.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        let tree_state = cx.new(|cx| TreeState::new(cx));

        // Search input state.
        let search_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions..."));

        // Build initial tree items.
        let items = build_tree_items(store.read(cx).sessions(), "");
        tree_state.update(cx, |state, cx| state.set_items(items, cx));

        // Observe store → rebuild tree khi sessions thay đổi (apply search filter).
        cx.observe(&store, |this, store, cx| {
            let query = this.search_state.read(cx).value().to_string();
            let items = build_tree_items(store.read(cx).sessions(), &query);
            this.tree_state
                .update(cx, |state, cx| state.set_items(items, cx));
            this.right_clicked_ix.set(None);
            cx.notify();
        })
        .detach();

        // Observe search input → debounce 300ms → rebuild tree với filter.
        cx.observe(&search_state, |this, _state, cx| {
            // Thay task cũ = cancel (drop Task = cancel) → debounce.
            this.search_debounce_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                _ = this.update(cx, |this, cx| {
                    let query = this.search_state.read(cx).value().to_string();
                    let items = build_tree_items(this.store.read(cx).sessions(), &query);
                    this.tree_state
                        .update(cx, |state, cx| state.set_items(items, cx));
                    this.right_clicked_ix.set(None);
                    cx.notify();
                });
            }));
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            store,
            tree_state,
            search_state,
            search_debounce_task: None,
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

// ── Trait impls ──────────────────────────────────────────────

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
