//! The [`Render`] impl for [`TerminalPanel`] and its action handlers.
//!
//! The handlers are fired by both the context menu (via `.action()` on menu
//! items) and global key bindings, both routed through the `.on_action`
//! registrations below.

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement, Render, Styled, Window, div,
};

use gpui_component::ActiveTheme as _;

use oneterm_actions::{
    CloseSpace, DuplicateSession, SplitDown, SplitLeft, SplitRight, SplitUp, TerminalClear,
    TerminalCopy, TerminalPaste, TerminalSelectAll,
};

use super::super::handlers::edit;
use super::super::space::{SplitDir, render_node};
use super::TerminalPanel;

impl TerminalPanel {
    /// Run an edit command (copy/paste/select-all/clear) on the active
    /// terminal's session, if the active Space has one.
    fn edit_active(&self, edit: edit::EditCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.active_view() {
            let session = view.read(cx).session.clone();
            edit(&session, window, cx);
        }
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.tree.active();
        let single = self.tree.is_single();
        let panel = cx.entity().downgrade();
        let body = render_node(self.tree.root(), active, single, panel, window, cx);
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            // Terminal context-menu action handlers — also fired by global key bindings.
            .on_action(cx.listener(|this, _: &DuplicateSession, w, cx| {
                this.duplicate_session(this.tree.active(), w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitRight, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Right, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitLeft, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Left, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitUp, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Up, w, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitDown, w, cx| {
                this.split_active_at(this.tree.active(), SplitDir::Down, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalCopy, w, cx| {
                this.edit_active(edit::copy_selection, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalPaste, w, cx| {
                this.edit_active(edit::paste_clipboard, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalSelectAll, w, cx| {
                this.edit_active(edit::select_all, w, cx)
            }))
            .on_action(cx.listener(|this, _: &TerminalClear, w, cx| {
                this.edit_active(edit::clear_screen, w, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseSpace, w, cx| {
                if this.tree.leaf_count() > 1 {
                    this.close_space(this.tree.active(), w, cx);
                }
            }))
            .child(body)
    }
}
