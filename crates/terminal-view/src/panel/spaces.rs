//! Space operations on a [`TerminalPanel`]'s tree: split, close (tab and
//! Space), fill an empty Space, activate a Space, the tab drag-drop move, and
//! the active-session publish into `AppState`.

use gpui::{AppContext as _, Context, Entity, Window};
use gpui_component::dock::DockPlacement;
use gpui_component::dock::panel_handle;
use gpui_component::notification::NotificationType;
use gpui_component::resizable::ResizableState;

use oneterm_state::AppState;

use super::TerminalPanel;
use super::duplicate::{Unplaced, close_unplaced};
use super::terminal_panel::DEFAULT_TAB_TITLE;
use crate::space::{
    CloseOutcome, DragTerminalTab, SpaceContent, SpaceId, SpaceLeaf, SpaceTree, SplitDir,
};
use crate::terminal_view::TerminalView;

impl TerminalPanel {
    /// Split Space `space_id` in `dir`; the new empty Space becomes active.
    pub(crate) fn split_active_at(
        &mut self,
        space_id: SpaceId,
        dir: SplitDir,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new_id = self.split_tree(space_id, dir, cx);
        self.set_active_space(new_id, window, cx);
        cx.notify();
    }

    /// Split Space `space_id` in `dir` and return the new empty Space's id.
    /// The caller decides what happens to it (activate it, or fill it).
    pub(super) fn split_tree(
        &mut self,
        space_id: SpaceId,
        dir: SplitDir,
        cx: &mut Context<Self>,
    ) -> SpaceId {
        let new_id = self.tree.alloc_id();
        let empty = SpaceLeaf {
            id: new_id,
            content: SpaceContent::Empty,
            focus: cx.focus_handle(),
        };
        let state = cx.new(|_| ResizableState::default());
        self.tree.split(space_id, dir, empty, state);
        new_id
    }

    /// Close this terminal tab. The panel is removed from the dock when other
    /// tabs remain; the last tab is reset in place so the dock keeps a terminal
    /// tab to open new terminals into.
    pub(crate) fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_sibling = self
            .tab_panel
            .as_ref()
            .and_then(|tab_panel| tab_panel.upgrade())
            .is_some_and(|tabs| tabs.read(cx).panels().len() > 1);

        if has_sibling {
            let panel = cx.entity().clone();
            if let Some(dock_area) = dock_area(cx) {
                // Deferred: the dock is mid-dispatch of the action that closes
                // this panel, so it cannot be mutated during this frame.
                window.defer(cx, move |window, cx| {
                    dock_area.update(cx, |dock_area, cx| {
                        dock_area.remove_panel(panel, window, cx);
                    });
                });
            }
            return;
        }

        self.shutdown(window, cx);
        self.tree = SpaceTree::new_empty(cx.focus_handle());
        self.tab_title = DEFAULT_TAB_TITLE.to_string();
        self.tab_title_override = None;
        self.rebuild_title_subs(cx);
        self.focus_active_space(window, cx);
        self.republish_if_active(cx);
        cx.notify();
    }

    /// Close Space `space_id`. Resets the tab if it was the final Space.
    pub(crate) fn close_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (outcome, removed) = self.tree.close(space_id);
        if let Some(view) = removed {
            view.update(cx, |v, cx| v.shutdown(cx));
        }
        if outcome == CloseOutcome::LastSpaceClosed {
            self.close_tab(window, cx);
            return;
        }
        self.reactivate_after_tree_change(window, cx);
    }

    /// Spawn a local shell directly into empty Space `space_id`.
    pub(crate) fn new_terminal_here(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = Self::spawn_local_view(&self.deps, None, window, cx) else {
            log::warn!("new_terminal_here: spawn failed");
            return;
        };
        if let Err(view) = self.place_view(space_id, view, window, cx) {
            close_unplaced(
                Unplaced::View(view),
                NotificationType::Warning,
                "The destination Space is no longer available.",
                window,
                cx,
            );
        }
    }

    /// Install `view` into the empty Space `target`: wire its split context,
    /// fill the Space, resubscribe to title changes, and make it the active
    /// Space. On `Err` the Space was no longer empty and the view is handed
    /// back untouched (still alive) for the caller to place elsewhere or close.
    pub(super) fn place_view(
        &mut self,
        target: SpaceId,
        view: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Entity<TerminalView>> {
        self.attach_split_ctx(&view, target, cx);
        self.tree.fill_empty(target, view)?;
        self.rebuild_title_subs(cx);
        self.set_active_space(target, window, cx);
        cx.notify();
        Ok(())
    }

    /// Make Space `space_id` the active Space (focus it + refresh status bar).
    pub(crate) fn set_active_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tree.has_leaf(space_id) {
            return;
        }
        let changed = self.tree.active() != space_id;
        self.tree.set_active(space_id);
        self.focus_active_space(window, cx);
        if changed {
            self.republish_if_active(cx);
            cx.notify();
        }
    }

    /// Focus whatever the active Space holds: the terminal view, or the empty
    /// placeholder's own handle.
    pub(super) fn focus_active_space(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.tree.active_focus_handle(cx) {
            focus.focus(window, cx);
        }
    }

    /// Refresh the title subscriptions and re-focus the tree's active Space
    /// after a structural change (close / take).
    fn reactivate_after_tree_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_title_subs(cx);
        let active = self.tree.active();
        self.set_active_space(active, window, cx);
        cx.notify();
    }

    /// Take the active Space's terminal view out of this tree, leaving it empty
    /// (and collapsing that Space if other Spaces remain). Used by drag-drop.
    pub(super) fn take_active_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<TerminalView>> {
        let id = self.tree.active();
        let view = self.tree.take_leaf_terminal(id)?;
        if self.tree.leaf_count() > 1 {
            let _ = self.tree.close(id);
            self.reactivate_after_tree_change(window, cx);
        }
        Some(view)
    }

    /// Handle a Terminal Tab dropped onto empty Space `target`: move the source
    /// tab's active terminal into this Space.
    pub(crate) fn handle_tab_drop(
        &mut self,
        target: SpaceId,
        drag: &DragTerminalTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = drag.panel.upgrade() else {
            return;
        };
        // Verify the destination BEFORE taking the terminal out of the source:
        // once taken, a failed `fill_empty` would leave the live session
        // orphaned. Everything below runs synchronously on the UI thread, so a
        // Space that is empty here is still empty at `fill_empty`.
        if !self.tree.has_leaf(target) || self.tree.leaf_terminal(target).is_some() {
            return;
        }
        let is_self = source == cx.entity();

        let view = if is_self {
            // Dropping within the same tab: no-op for a single Space.
            if self.tree.leaf_count() == 1 {
                return;
            }
            self.take_active_terminal_view(window, cx)
        } else {
            source.update(cx, |panel, cx| panel.take_active_terminal_view(window, cx))
        };
        let Some(view) = view else {
            return;
        };

        if let Err(view) = self.place_view(target, view, window, cx) {
            // Cannot happen after the guard above; if it ever does, hand the
            // terminal back to the source's active (just emptied) Space rather
            // than destroying the user's session.
            log::error!("handle_tab_drop: target Space is no longer empty; restoring source");
            if is_self {
                self.restore_dropped_view(view, window, cx);
            } else {
                source.update(cx, |panel, cx| panel.restore_dropped_view(view, window, cx));
            }
            return;
        }

        // Remove the emptied source tab (only when the source is a different,
        // now-terminal-less panel).
        if !is_self
            && source.read(cx).has_no_terminals()
            && let Some(dock_area) = dock_area(cx)
        {
            dock_area.update(cx, |dock_area, cx| {
                dock_area.remove_panel(source.clone(), window, cx);
            });
        }
        cx.notify();
    }

    /// Put a terminal view taken by `handle_tab_drop` back into this panel's
    /// active Space (the one `take_active_terminal_view` just emptied). Last
    /// resort only — the drop path guards the destination before taking.
    fn restore_dropped_view(
        &mut self,
        view: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let home = self.tree.active();
        if let Err(view) = self.place_view(home, view, window, cx) {
            // Nowhere left to place it: release the session explicitly.
            log::error!("handle_tab_drop: source Space is no longer empty; closing terminal");
            view.update(cx, |view, cx| view.shutdown(cx));
            cx.notify();
        }
    }

    /// Add `panel` to the dock's center area as a new terminal tab.
    pub(super) fn add_tab_to_dock(
        panel: gpui::Entity<TerminalPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dock_area) = dock_area(cx) else {
            return;
        };
        dock_area.update(cx, |dock_area, cx| {
            dock_area.add_panel_view(panel_handle(panel), DockPlacement::Center, None, window, cx);
        });
    }

    /// Publish the active Space's session into `AppState` (SFTP / cwd / locality)
    /// when this panel is the selected tab; a background tab must not overwrite
    /// what the selected one published.
    pub(super) fn republish_if_active(&mut self, cx: &mut Context<Self>) {
        if self.is_active {
            self.publish_active_session(cx);
        }
    }

    /// Publish the active Space's session into `AppState` (SFTP / cwd / locality).
    pub(super) fn publish_active_session(&mut self, cx: &mut Context<Self>) {
        let (sftp, cwd_source, is_local) = match self.tree.active_terminal() {
            Some(view) => {
                let session = view.read(cx).session.read(cx);
                let capabilities = session.capabilities();
                (
                    capabilities.sftp,
                    capabilities.cwd_source,
                    session.kind().is_local(),
                )
            }
            None => (None, None, true),
        };
        let workspace_id = self.workspace_id;
        AppState::global(cx).update(cx, |state, cx| {
            state.set_active_workspace(workspace_id, sftp, cwd_source, is_local);
            cx.notify();
        });
    }
}

/// The live workspace dock area, if the app still has one.
fn dock_area(cx: &gpui::App) -> Option<gpui::Entity<gpui_component::dock::DockArea>> {
    AppState::global(cx)
        .read(cx)
        .dock_area
        .clone()
        .and_then(|dock_area| dock_area.upgrade())
}
