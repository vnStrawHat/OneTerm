//! [`TerminalPanel`] — leaf panel hiển thị 1 Terminal session.
//!
//! Skeleton: chỉ render placeholder text ở giữa. Sau này sẽ render
//! terminal emulator (ANSI/VT, scrollback) qua `core::terminal`.

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme,
    dock::{Panel, PanelControl, PanelEvent},
};

/// Panel hiển thị 1 Terminal session.
///
/// `panel_name = "terminal"` — dùng để serialize/deserialize layout
/// (xem `docs/gui-layout.md` §5).
pub struct TerminalPanel {
    focus_handle: FocusHandle,
}

impl TerminalPanel {
    /// Tạo panel mới.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    /// Helper tạo `Entity<Self>` (dùng cho `DockItem::tab` + `register_panel`).
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Terminal"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No terminal session. Press Ctrl+N to open.")
    }
}
