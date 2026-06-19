//! [`TerminalPanel`] — leaf panel hiển thị 1 Terminal session.
//!
//! MVP: tự tạo `LocalSession` (cmd mặc định) + `LocalTerminalView`.
//! TODO: chuyển construction session ra app layer để SSH pluggable (View vẫn
//! dùng `dyn TerminalSession`, chỉ đổi factory).

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme,
    dock::{Panel, PanelControl, PanelEvent},
};
use myterm2_core::TerminalSession;
use myterm2_local::{LocalSession, PtySize};

use crate::state::TerminalSettings;

use super::terminal_view::LocalTerminalView;

/// Panel hiển thị 1 Terminal session.
pub struct TerminalPanel {
    focus_handle: FocusHandle,
    view: Entity<LocalTerminalView>,
}

impl TerminalPanel {
    /// Tạo panel + spawn session local mặc định (cmd trên Windows).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let shell = TerminalSettings::global(cx).read(cx).shell.clone();
        let session: Entity<Box<dyn TerminalSession>> = cx.new(|_cx| {
            Box::new(
                LocalSession::spawn(shell, PtySize { rows: 24, cols: 80 })
                    .expect("spawn local session"),
            ) as Box<dyn TerminalSession>
        });
        let view = cx.new(|cx| LocalTerminalView::new(session, window, cx));
        Self { focus_handle, view }
    }

    /// Helper tạo `Entity<Self>`.
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
            .bg(cx.theme().background)
            .child(self.view.clone())
    }
}