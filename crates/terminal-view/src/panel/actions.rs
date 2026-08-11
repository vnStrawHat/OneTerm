//! Terminal context-menu action handlers and the [`Render`] impl for
//! [`TerminalPanel`].
//!
//! The action handlers are fired by both the context menu (via `.action()` on
//! menu items) and global key bindings (registered in the `render` method via
//! `.on_action`).

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement, Render, Styled, Window, div,
};

use gpui_component::ActiveTheme as _;

use oneterm_actions::{
    CloseSpace, DuplicateSession, SplitDown, SplitLeft, SplitRight, SplitUp, TerminalClear,
    TerminalCopy, TerminalPaste, TerminalSelectAll,
};

use super::super::space::{SplitDir, render_node};
use super::TerminalPanel;

// ── Terminal context-menu action handlers ────────────────────────────

impl TerminalPanel {
    /// Duplicate the session in the active terminal Space.
    fn on_action_duplicate_session(
        &mut self,
        _: &DuplicateSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate_session(self.tree.active(), window, cx);
    }

    /// Split the active terminal Space to the right.
    fn on_action_split_right(
        &mut self,
        _: &SplitRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.split_active_at(self.tree.active(), SplitDir::Right, window, cx);
    }

    /// Split the active terminal Space to the left.
    fn on_action_split_left(&mut self, _: &SplitLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.split_active_at(self.tree.active(), SplitDir::Left, window, cx);
    }

    /// Split the active terminal Space upward.
    fn on_action_split_up(&mut self, _: &SplitUp, window: &mut Window, cx: &mut Context<Self>) {
        self.split_active_at(self.tree.active(), SplitDir::Up, window, cx);
    }

    /// Split the active terminal Space downward.
    fn on_action_split_down(&mut self, _: &SplitDown, window: &mut Window, cx: &mut Context<Self>) {
        self.split_active_at(self.tree.active(), SplitDir::Down, window, cx);
    }

    /// Copy the terminal selection to the clipboard.
    fn on_action_terminal_copy(
        &mut self,
        _: &TerminalCopy,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.active_view() {
            let session = view.read(cx).session.clone();
            if let Some(text) = session.read(cx).selection_text() {
                if !text.is_empty() {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                }
            }
        }
    }

    /// Paste the clipboard contents into the active terminal.
    fn on_action_terminal_paste(
        &mut self,
        _: &TerminalPaste,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.active_view() {
            let session = view.read(cx).session.clone();
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    session.update(cx, |s, _| s.paste(&text));
                }
            }
        }
    }

    /// Select all text in the active terminal.
    fn on_action_terminal_select_all(
        &mut self,
        _: &TerminalSelectAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.active_view() {
            let session = view.read(cx).session.clone();
            session.update(cx, |s, _| s.select_all());
        }
    }

    /// Clear the active terminal screen.
    fn on_action_terminal_clear(
        &mut self,
        _: &TerminalClear,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self.active_view() {
            let session = view.read(cx).session.clone();
            session.update(cx, |s, _| s.clear());
        }
    }

    /// Close the active terminal Space (not the whole tab).
    fn on_action_close_space(
        &mut self,
        _: &CloseSpace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tree.leaf_count() > 1 {
            self.close_space(self.tree.active(), window, cx);
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
            .on_action(cx.listener(Self::on_action_duplicate_session))
            .on_action(cx.listener(Self::on_action_split_right))
            .on_action(cx.listener(Self::on_action_split_left))
            .on_action(cx.listener(Self::on_action_split_up))
            .on_action(cx.listener(Self::on_action_split_down))
            .on_action(cx.listener(Self::on_action_terminal_copy))
            .on_action(cx.listener(Self::on_action_terminal_paste))
            .on_action(cx.listener(Self::on_action_terminal_select_all))
            .on_action(cx.listener(Self::on_action_terminal_clear))
            .on_action(cx.listener(Self::on_action_close_space))
            .child(body)
    }
}
