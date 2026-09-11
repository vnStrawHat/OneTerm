//! Drag payloads for moving rows between the Local and Remote panes, plus the
//! floating preview shown under the cursor while dragging.

use std::path::PathBuf;

use gpui::{Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div};
use gpui_component::ActiveTheme as _;
use oneterm_core::FileEntry;

/// A local row being dragged (drop on the remote list = upload).
#[derive(Clone, Debug)]
pub(crate) struct LocalRowDrag {
    pub path: PathBuf,
    pub name: String,
}

/// A remote row being dragged (drop on the local list = download).
#[derive(Clone, Debug)]
pub(crate) struct RemoteRowDrag {
    pub entry: FileEntry,
}

/// The name of the dragged entry, drawn under the cursor.
pub(crate) struct DragPreview {
    label: String,
}

impl DragPreview {
    pub(crate) fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
        }
    }
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background)
            .text_sm()
            .text_color(theme.foreground)
            .child(self.label.clone())
    }
}
