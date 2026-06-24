//! [`TerminalPanel`] — leaf panel hiển thị 1 Terminal session.
//!
//! MVP: tự tạo `LocalSession` (cmd mặc định) + `LocalTerminalView`.
//! TODO: chuyển construction session ra app layer để SSH pluggable (View vẫn
//! dùng `dyn TerminalSession`, chỉ đổi factory).

use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
};
use myterm2_core::TerminalSession;
use myterm2_local::{LocalSession, PtySize};

use crate::state::TerminalSettings;

use super::view::LocalTerminalView;

/// Panel hiển thị 1 Terminal session.
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    /// Tham chiếu tới `TabPanel` chứa panel này — dùng cho nút close tab.
    tab_panel: Option<WeakEntity<TabPanel>>,
}

impl TerminalPanel {
    /// Tạo panel + spawn session local mặc định (cmd trên Windows).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (shell, scrollback_history) = {
            let settings = TerminalSettings::global(cx).read(cx);
            (settings.shell.clone(), settings.scrollback_history)
        };
        let session: Entity<Box<dyn TerminalSession>> = cx.new(|_cx| {
            Box::new(
                LocalSession::spawn(shell, PtySize { rows: 24, cols: 80 }, scrollback_history)
                    .expect("spawn local session"),
            ) as Box<dyn TerminalSession>
        });
        let view = cx.new(|cx| LocalTerminalView::new(session, window, cx));
        // Focus terminal view ngay khi tạo — app startup + new tab.
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
        }
    }

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl Focusable for TerminalPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // Delegate to terminal view — khi dock area focus panel,
        // terminal view bên trong nhận focus.
        self.view.read(cx).focus_handle(cx)
    }
}

impl Panel for TerminalPanel {
    fn panel_name(&self) -> &'static str {
        "terminal"
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_panel = self.tab_panel.clone();
        let panel_entity = cx.entity().clone();
        let theme = cx.theme().muted_foreground;

        h_flex()
            .w_full()
            .min_w(px(120.))
            .items_center()
            .gap_1()
            // Bù padding phải 12px của Tab inner_h_flex để × sát viền phải.
            .mr(-px(12.))
            // Tiêu đề "Terminal" — co giãn, cắt bớt bằng ellipsis nếu hẹp.
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child("Terminal"),
            )
            // Nút close (×) — sát bên phải tab.
            .when_some(tab_panel, |this, tp| {
                this.child(
                    div()
                        .id("tab-close")
                        .flex_shrink_0()
                        .cursor_pointer()
                        .size_4()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.))
                        .hover(move |this| this.bg(theme.opacity(0.15)))
                        // Ngăn click lan ra Tab (tránh activate tab).
                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            if let Some(tp) = tp.upgrade() {
                                let panel: Arc<dyn PanelView> = Arc::new(panel_entity.clone());
                                tp.update(cx, |tp, cx| {
                                    tp.remove_panel(panel, window, cx);
                                });
                            }
                        })
                        .child(
                            Icon::new(IconName::Close)
                                .xsmall()
                                .text_color(theme),
                        ),
                )
            })
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }

    fn on_added_to(
        &mut self,
        tab_panel: WeakEntity<TabPanel>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.tab_panel = Some(tab_panel);
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel")
            .size_full()
            .bg(cx.theme().background)
            .child(self.view.clone())
    }
}