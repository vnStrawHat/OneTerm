//! [`DragTerminalTab`] — the public drag payload for moving a Terminal Tab into
//! an empty Space.
//!
//! gpui-component's own `DragPanel` is `pub(crate)`, so the `ui` crate cannot
//! intercept the dock's native tab drag. Instead the tab title (rendered by
//! `TerminalPanel::title`) emits this payload, and an empty Space accepts it.
//!
//! See `docs/terminal-split/03-drag-drop.md`.

use gpui::{
    Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, WeakEntity,
    Window, div,
};
use gpui_component::{ActiveTheme as _, dock::TabPanel};

use super::super::panel::TerminalPanel;

/// Payload dragged from a Terminal Tab title onto an empty Space.
#[derive(Clone)]
pub struct DragTerminalTab {
    /// The source terminal panel being dragged.
    pub panel: WeakEntity<TerminalPanel>,
    /// The `TabPanel` the source lives in (to remove it after a successful move).
    pub tab_panel: WeakEntity<TabPanel>,
    /// The tab label — shown in the small drag preview.
    pub title: SharedString,
}

impl Render for DragTerminalTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .bg(cx.theme().background)
            .border_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(self.title.clone())
    }
}
