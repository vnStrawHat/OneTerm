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

use crate::actions::{
    CloseSpace, SplitDown, SplitLeft, SplitRight, SplitUp, TerminalClear, TerminalCopy,
    TerminalPaste, TerminalSelectAll,
};

use super::super::space::{SplitDir, render_node};
use super::TerminalPanel;

// ── Terminal context-menu action handlers ────────────────────────────

impl TerminalPanel {
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{resolve_tab_label, trim_path_title};

    #[test]
    fn live_title_is_used() {
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "user@host:24"),
            "user@host: ~/repo"
        );
        assert_eq!(resolve_tab_label(Some("cmd.exe"), "Terminal"), "cmd.exe");
    }

    #[test]
    fn none_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(None, "Terminal"), "Terminal");
        assert_eq!(resolve_tab_label(None, "prod-server"), "prod-server");
    }

    #[test]
    fn empty_title_falls_back_to_static_label() {
        assert_eq!(resolve_tab_label(Some(""), "Terminal"), "Terminal");
    }

    #[test]
    fn fallback_is_returned_by_value() {
        let label = resolve_tab_label(None, "Terminal");
        assert_eq!(label, "Terminal");
    }

    #[test]
    fn windows_drive_path_shortened_to_basename() {
        assert_eq!(
            resolve_tab_label(Some("C:\\Windows\\system32\\cmd.exe"), "Terminal"),
            "cmd.exe"
        );
        assert_eq!(
            resolve_tab_label(
                Some("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
                "Terminal"
            ),
            "powershell.exe"
        );
    }

    #[test]
    fn posix_path_shortened_to_basename() {
        assert_eq!(resolve_tab_label(Some("/usr/bin/bash"), "Terminal"), "bash");
        assert_eq!(resolve_tab_label(Some("/bin/sh"), "Terminal"), "sh");
    }

    #[test]
    fn relative_or_descriptive_titles_not_trimmed() {
        assert_eq!(resolve_tab_label(Some("~/repo"), "Terminal"), "~/repo");
        assert_eq!(
            resolve_tab_label(Some("user@host: ~/repo"), "Terminal"),
            "user@host: ~/repo"
        );
        assert_eq!(
            resolve_tab_label(Some("vim — main.rs"), "Terminal"),
            "vim — main.rs"
        );
    }

    #[test]
    fn trim_path_title_helper_directly() {
        assert_eq!(trim_path_title("C:\\Windows\\system32\\cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("/usr/bin/bash"), "bash");
        assert_eq!(trim_path_title("cmd.exe"), "cmd.exe");
        assert_eq!(trim_path_title("user@host: ~/repo"), "user@host: ~/repo");
        assert_eq!(trim_path_title("  /usr/bin/zsh  "), "zsh");
    }

    #[test]
    fn phase1_terminal_titles_are_sanitized_by_policy() {
        use oneterm_core::terminal::security_policy::TerminalSecurityPolicy;

        let policy = TerminalSecurityPolicy::default();

        // Control characters stripped.
        let controlled = "safe\u{0007}\u{001b}[31m\u{202e}txt.exe";
        let sanitized = policy.sanitize_title(controlled).unwrap();
        assert_eq!(sanitized, "safe[31mtxt.exe");

        // Oversized title truncated.
        let oversized = "x".repeat(256 * 1024);
        let sanitized = policy.sanitize_title(&oversized).unwrap();
        assert!(sanitized.len() <= 4 * 1024);

        // resolve_tab_label still passes through what it receives.
        let clean = "vim — main.rs";
        assert_eq!(resolve_tab_label(Some(clean), "Terminal"), clean);
    }
}
