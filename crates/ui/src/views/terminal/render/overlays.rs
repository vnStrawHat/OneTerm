//! Overlay helpers cho `LocalTerminalView::render`.

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px,
};
use gpui_component::Theme;

use super::LocalTerminalView;

impl LocalTerminalView {
    /// Bell indicator overlay (góc trên-phải).
    pub(crate) fn bell_overlay(
        &self,
        has_bell: bool,
        bell_enabled: bool,
        theme_ref: &Theme,
    ) -> Option<impl IntoElement> {
        if has_bell && bell_enabled {
            Some(
                div()
                    .id("terminal-bell")
                    .absolute()
                    .top_1()
                    .right_2()
                    .px_1()
                    .py_0()
                    .text_xs()
                    .text_color(theme_ref.warning)
                    .child("🔔"),
            )
        } else {
            None
        }
    }

    /// Vi mode indicator (góc trên-trái).
    pub(crate) fn vi_mode_overlay(&self, theme_ref: &Theme) -> Option<impl IntoElement> {
        if self.vi_mode {
            Some(
                div()
                    .id("terminal-vi-mode")
                    .absolute()
                    .top_1()
                    .left_2()
                    .px_2()
                    .py_0p5()
                    .text_xs()
                    .rounded_sm()
                    .bg(theme_ref.accent.opacity(0.8))
                    .text_color(theme_ref.foreground)
                    .child(if self.vi_selecting {
                        "-- VISUAL --"
                    } else {
                        "-- NORMAL --"
                    }),
            )
        } else {
            None
        }
    }

    /// Vi mode cursor overlay.
    pub(crate) fn vi_cursor_overlay(&self, theme_ref: &Theme) -> Option<impl IntoElement> {
        if !self.vi_mode {
            return None;
        }
        let m = *self.metrics.borrow();
        let cw = f32::from(m.cell_width);
        let lh = f32::from(m.line_height);
        if cw <= 0.0 || lh <= 0.0 {
            return None;
        }
        let bounds = m.bounds?;
        let x = f32::from(bounds.origin.x + m.gutter_width) + self.vi_cursor.1 as f32 * cw;
        let y = f32::from(bounds.origin.y) + self.vi_cursor.0 as f32 * lh;
        Some(
            div()
                .id("vi-cursor")
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(cw))
                .h(px(lh))
                .border_1()
                .border_color(theme_ref.accent)
                .rounded_sm(),
        )
    }

    /// Breadcrumb bar (bottom) — cwd path từ OSC 7.
    pub(crate) fn breadcrumb_overlay(
        &self,
        session: &gpui::Entity<Box<dyn oneterm_core::TerminalSession>>,
        theme_ref: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let _ = self;
        let breadcrumb = session.read(cx).breadcrumb_text();
        let fg_process = session.read(cx).foreground_process();
        breadcrumb.map(|bc| {
            let label = if let Some(proc) = &fg_process {
                format!("{} — {}", proc, bc)
            } else {
                bc
            };
            div()
                .id("terminal-breadcrumb")
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(20.0))
                .flex()
                .items_center()
                .px_2()
                .text_xs()
                .text_color(theme_ref.border)
                .bg(theme_ref.background.opacity(0.9))
                .child(label)
        })
    }
}
