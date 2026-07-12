//! Space operations for [`TerminalPanel`] — split, close, fill, drag-drop, and
//! the active-session publish logic.
//!
//! These methods are part of the `impl TerminalPanel` block split out of
//! [`super`] to keep each file under the ~400-line guideline.

use std::sync::Arc;

use gpui::{AppContext as _, Entity, Window};

use gpui_component::{dock::PanelView, resizable::ResizableState};

use crate::state::{AppState, TerminalSettings};

use super::super::space::{
    CloseOutcome, DragTerminalTab, SpaceContent, SpaceId, SpaceLeaf, SplitDir,
};
use super::super::view::LocalTerminalView;
use super::TerminalPanel;

impl TerminalPanel {
    /// Split Space `space_id` in `dir`; the new empty Space becomes active.
    pub fn split_active_at(
        &mut self,
        space_id: SpaceId,
        dir: SplitDir,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let new_id = self.tree.alloc_id();
        let empty = SpaceLeaf {
            id: new_id,
            content: SpaceContent::Empty,
            focus: cx.focus_handle(),
        };
        let state = cx.new(|_| ResizableState::default());
        self.tree.split(space_id, dir, empty, state);
        self.set_active_space(new_id, window, cx);
        cx.notify();
    }

    /// Close Space `space_id`. Closes the whole tab if it was the last Space.
    pub fn close_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (outcome, removed) = self.tree.close(space_id);
        if let Some(view) = removed {
            view.read(cx).session.read(cx).close();
        }
        if outcome == CloseOutcome::LastSpaceClosed {
            if let Some(tp) = self.tab_panel.as_ref().and_then(|w| w.upgrade()) {
                let panel: Arc<dyn PanelView> = Arc::new(cx.entity());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
            return;
        }
        self.rebuild_title_subs(cx);
        let active = self.tree.active();
        self.set_active_space(active, window, cx);
        cx.notify();
    }

    /// Spawn a local shell directly into empty Space `space_id`.
    pub fn new_terminal_here(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = TerminalPanel::spawn_local_view(window, cx);
        self.attach_split_ctx(&view, space_id, cx);
        self.tree.fill_empty(space_id, view);
        self.rebuild_title_subs(cx);
        self.set_active_space(space_id, window, cx);
        cx.notify();
    }

    /// Make Space `space_id` the active Space (focus it + refresh status bar).
    pub fn set_active_space(
        &mut self,
        space_id: SpaceId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.tree.has_leaf(space_id) {
            return;
        }
        let changed = self.tree.active() != space_id;
        self.tree.set_active(space_id);
        if let Some(fh) = self.tree.active_focus_handle(cx) {
            fh.focus(window, cx);
        }
        if changed {
            if self.is_active {
                self.publish_active_session(window, cx);
            }
            cx.notify();
        }
    }

    /// Take the active Space's terminal view out of this tree, leaving it empty
    /// (and collapsing that Space if other Spaces remain). Used by drag-drop.
    pub fn take_active_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<LocalTerminalView>> {
        let id = self.tree.active();
        let view = self.tree.take_leaf_terminal(id)?;
        if self.tree.leaf_count() > 1 {
            let _ = self.tree.close(id);
            self.rebuild_title_subs(cx);
            let active = self.tree.active();
            self.set_active_space(active, window, cx);
            cx.notify();
        }
        Some(view)
    }

    /// Handle a Terminal Tab dropped onto empty Space `target`: move the source
    /// tab's active terminal into this Space (see `docs/terminal-split/03`).
    pub fn handle_tab_drop(
        &mut self,
        target: SpaceId,
        drag: &DragTerminalTab,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(src) = drag.panel.upgrade() else {
            return;
        };
        let is_self = src == cx.entity();

        let view = if is_self {
            // Dropping within the same tab: no-op for a single Space.
            if self.tree.leaf_count() == 1 {
                return;
            }
            self.take_active_terminal_view(window, cx)
        } else {
            src.update(cx, |sp, cx| sp.take_active_terminal_view(window, cx))
        };
        let Some(view) = view else {
            return;
        };

        self.attach_split_ctx(&view, target, cx);
        self.tree.fill_empty(target, view);
        self.rebuild_title_subs(cx);
        self.set_active_space(target, window, cx);

        // Remove the emptied source tab (only when the source is a different,
        // now-terminal-less panel).
        if !is_self && src.read(cx).has_no_terminals(cx) {
            if let Some(tp) = drag.tab_panel.upgrade() {
                let panel: Arc<dyn PanelView> = Arc::new(src.clone());
                tp.update(cx, |tp, cx| {
                    tp.remove_panel(panel, window, cx);
                });
            }
        }
        cx.notify();
    }

    /// Publish the active Space's session into `AppState` (SFTP / cwd / locality)
    /// and apply the auto-hide-right-dock rule.
    pub(super) fn publish_active_session(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (sftp, cwd_source, is_local) = match self.tree.active_terminal() {
            Some(view) => {
                let s = view.read(cx).session.read(cx);
                (s.sftp(), s.cwd_source(), s.is_local())
            }
            None => (None, None, true),
        };
        AppState::global(cx).update(cx, |state, cx| {
            state.active_sftp = sftp;
            state.active_cwd_source = cwd_source;
            state.active_is_local = is_local;
            cx.notify();
        });

        let auto_hide = TerminalSettings::global(cx)
            .read(cx)
            .auto_hide_right_dock_on_local;
        if auto_hide {
            let dock_area = AppState::global(cx)
                .read(cx)
                .dock_area
                .as_ref()
                .and_then(|w| w.upgrade());
            if let Some(dock_area) = dock_area {
                crate::layout::workspace::set_right_dock_open(&dock_area, !is_local, window, cx);
            }
        }
    }
}
