//! [`DragTerminalTab`] — the public drag payload for moving a Terminal Tab into
//! an empty Space.
//!
//! GPUI Kit 0.6 exposes its native `DragPanel`, but terminal Spaces use a
//! terminal-specific payload so the drop handler can move a Space-tree view and
//! render a terminal-title preview without coupling to dock internals.
//!
//! See `docs/terminal-split/03-drag-drop.md`.

use gpui::{
    Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _, WeakEntity,
    Window, div,
};
use gpui_component::ActiveTheme as _;

use super::super::panel::TerminalPanel;

/// Payload dragged from a Terminal Tab title onto an empty Space.
#[derive(Clone)]
pub struct DragTerminalTab {
    /// The source terminal panel being dragged.
    pub panel: WeakEntity<TerminalPanel>,
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
