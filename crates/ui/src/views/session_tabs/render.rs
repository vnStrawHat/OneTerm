//! `impl Render for SessionPanel` — header (search + new-session button),
//! empty/no-results states, và final div assembly.
//!
//! Tách từ `tabs.rs` để giảm độ dài file.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    menu::ContextMenuExt,
};

use crate::actions::NewSession;

use super::panel::SessionPanel;
use super::tree_builder::session_matches;

impl Render for SessionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let sessions = self.store.read(cx).sessions().to_vec();
        let focus = self.focus_handle.clone();
        let search_state = self.search_state.clone();

        // Search query hiện tại — kiểm tra có results hay không.
        let query = self.search_state.read(cx).value().to_string();
        let q = query.trim().to_lowercase();
        let has_results = if q.is_empty() {
            !sessions.is_empty()
        } else {
            sessions.iter().any(|s| session_matches(s, &q))
        };

        // Header: search input + new-session button.
        let header = h_flex()
            .w_full()
            .px_3()
            .py_2()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Input::new(&search_state)
                            .small()
                            .flex_1()
                            // Input mặc định border_1 (4 cạnh) → chỉ giữ border bottom.
                            .border_b_1()
                            .border_t_0()
                            .border_l_0()
                            .border_r_0()
                            // Nền trong suốt — bỏ bg mặc định của Input.
                            .bg(gpui::transparent_black()),
                    ),
            );

        // Empty state — không có session nào.
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

        // No-results state — có session nhưng search không match.
        let no_results = h_flex()
            .id("no-results")
            .w_full()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(theme.muted_foreground)
            .text_sm()
            .child(format!("No sessions found for \"{}\".", query.trim()));

        // Tree widget.
        let tree_widget = self.render_tree_widget();

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
                    .when(!sessions.is_empty() && !has_results, |t| {
                        t.child(no_results)
                    })
                    .when(!sessions.is_empty() && has_results, |t| {
                        t.child(tree_widget)
                    }),
            )
    }
}
