//! [`SessionPanel`] — leaf panel displaying the list of SSH sessions as a Tree.
//!
//! Renders a Tree (1 level: Group → Item, or Item at root) from `ssh_session.json`
//! (via [`crate::state::SshSessionStore`]) at startup.
//!
//! - Items without a group → shown at the root, on top (sorted by label).
//! - Items with a group → grouped into a folder by group name (sorted by group,
//!   then by label within the group).
//! - Double-click a session item → open the SSH connect dialog.
//! - Right-click an empty area of the panel → "New Session" context menu.
//! - Right-click a session item → context menu: Open, Delete, Property.
//! - Right-click a group folder → context menu: Property (rename group).
//! - "New Session" / "Property" → open a dialog (see [`super::session_dialog`]).
//! - "Open" / double-click → open the connect dialog (see [`super::connect_dialog`]).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Task,
    Window,
};
use gpui_component::{
    WindowExt,
    dock::{Panel, PanelControl, PanelEvent},
    input::InputState,
    notification::NotificationType,
    tree::TreeState,
};

use crate::actions::{DeleteSession, NewSession, OpenSession, SessionProperty};
use crate::state::SshSessionStore;

use super::session_dialog::open_session_dialog;
use super::tree_builder::build_tree_items;

/// Id prefix for leaf TreeItems (sessions) — encodes the store index.
pub(crate) const SESSION_ID_PREFIX: &str = "session:";
/// Id prefix for folder TreeItems (groups).
pub(crate) const GROUP_ID_PREFIX: &str = "group:";

/// Panel displaying the list of SSH sessions as a Tree.
///
/// `panel_name = "session"`.
pub struct SessionPanel {
    pub(crate) focus_handle: FocusHandle,
    pub(crate) store: Entity<SshSessionStore>,
    pub(crate) tree_state: Entity<TreeState>,
    /// Search input state — filters sessions by label/host/user/group.
    pub(crate) search_state: Entity<InputState>,
    /// Debounce task for search — replacing the old task cancels it (debounce).
    pub(crate) search_debounce_task: Option<Task<()>>,
    /// Tracks the clicked index (any button) for highlighting — only one item at a time.
    pub(crate) right_clicked_ix: Rc<Cell<Option<usize>>>,
}

impl SessionPanel {
    /// Create a new panel — bind to the global [`SshSessionStore`] and observe it
    /// to rebuild the tree when the session list changes.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = SshSessionStore::global(cx);
        let tree_state = cx.new(|cx| TreeState::new(cx));

        // Search input state.
        let search_state =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions..."));

        // Build initial tree items.
        let items = build_tree_items(store.read(cx).sessions(), "");
        tree_state.update(cx, |state, cx| state.set_items(items, cx));

        // Observe the store → rebuild the tree when sessions change (apply the search filter).
        cx.observe(&store, |this, store, cx| {
            let query = this.search_state.read(cx).value().to_string();
            let items = build_tree_items(store.read(cx).sessions(), &query);
            this.tree_state
                .update(cx, |state, cx| state.set_items(items, cx));
            this.right_clicked_ix.set(None);
            cx.notify();
        })
        .detach();

        // Observe the search input → debounce 300ms → rebuild the tree with the filter.
        cx.observe(&search_state, |this, _state, cx| {
            // Replacing the old task cancels it (dropping a Task = cancel) → debounce.
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

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Action handler: open the "New SSH Session" dialog (create new).
    pub(crate) fn on_new_session(
        &mut self,
        _: &NewSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_session_dialog(window, cx, None);
    }

    /// Resolve the store index of the currently selected session in the tree.
    fn selected_session_ix(&self, cx: &App) -> Option<usize> {
        let item = self.tree_state.read(cx).selected_item()?;
        super::tree_builder::parse_session_id(&item.id)
    }

    /// Action handler: open the connect dialog for the selected session.
    pub(crate) fn on_open_session(
        &mut self,
        _: &OpenSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.selected_session_ix(cx) {
            if let Some(s) = self.store.read(cx).sessions().get(ix).cloned() {
                super::connect_dialog::open_connect_dialog(s, ix, window, cx);
            }
        }
    }

    /// Action handler: delete the selected session from the store.
    pub(crate) fn on_delete_session(
        &mut self,
        _: &DeleteSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.selected_session_ix(cx) {
            self.store.update(cx, |s, cx| {
                s.remove(ix, cx);
            });
            window.push_notification(
                crate::notif_ext::notify(NotificationType::Success, "SSH session deleted.", cx),
                cx,
            );
        }
    }

    /// Action handler: open the property dialog for the selected session.
    pub(crate) fn on_session_property(
        &mut self,
        _: &SessionProperty,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.selected_session_ix(cx) {
            if let Some(s) = self.store.read(cx).sessions().get(ix).cloned() {
                open_session_dialog(window, cx, Some((ix, s)));
            }
        }
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
