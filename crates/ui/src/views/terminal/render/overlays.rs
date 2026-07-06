//! Overlay helpers for `LocalTerminalView::render`.

use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px, relative,
};
use gpui_component::Theme;
use oneterm_core::TerminalProgress;

use super::LocalTerminalView;

impl LocalTerminalView {
    /// Taskbar progress overlay (OSC 9;4) — a thin bar along the top edge.
    /// The fill width follows the reported percent; the color reflects the
    /// state (normal/error/paused). Indeterminate shows a full-width bar.
    pub(crate) fn progress_overlay(&self, theme_ref: &Theme) -> Option<impl IntoElement> {
        let progress = self.progress?;
        let (fraction, color) = match progress {
            TerminalProgress::Remove => return None,
            TerminalProgress::Set(pct) => (pct as f32 / 100.0, theme_ref.blue),
            TerminalProgress::Error(pct) => (pct as f32 / 100.0, theme_ref.danger),
            TerminalProgress::Paused(pct) => (pct as f32 / 100.0, theme_ref.warning),
            // Indeterminate: no known percent → fill the whole track.
            TerminalProgress::Indeterminate => (1.0, theme_ref.blue),
        };
        Some(
            div()
                .id("terminal-progress")
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(5.0))
                .bg(theme_ref.muted.opacity(0.4))
                .child(
                    div()
                        .id("terminal-progress-fill")
                        .h_full()
                        .w(relative(fraction.clamp(0.0, 1.0)))
                        .bg(color),
                ),
        )
    }

    /// Bell indicator overlay (top-right corner).
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
}
