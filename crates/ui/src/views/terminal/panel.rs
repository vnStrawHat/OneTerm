//! [`TerminalPanel`] — leaf panel hiển thị 1 Terminal session.
//!
//! MVP: tự tạo `LocalSession` (cmd mặc định) + `LocalTerminalView`.
//! TODO: chuyển construction session ra app layer để SSH pluggable (View vẫn
//! dùng `dyn TerminalSession`, chỉ đổi factory).

use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent, PanelView, TabPanel},
    h_flex,
};
use oneterm_core::TerminalSession;
use oneterm_local::{LocalSession, PtySize};

use crate::state::{AppState, TerminalSettings};

use super::view::LocalTerminalView;

/// Panel hiển thị 1 Terminal session.
pub struct TerminalPanel {
    view: Entity<LocalTerminalView>,
    /// Tham chiếu tới `TabPanel` chứa panel này — dùng cho nút close tab.
    tab_panel: Option<WeakEntity<TabPanel>>,
    /// Panel này có đang là tab được chọn trong `TabPanel` hay không.
    ///
    /// Không thể đọc `TabPanel` trong `title()` (lúc đó nó đang render) nên ta
    /// mirror trạng thái này qua hook [`Panel::set_active`], được `TabPanel`
    /// gọi mỗi khi tab active đổi.
    is_active: bool,
    /// Tiêu đề tab — "Terminal" cho local, session label cho SSH.
    tab_title: String,
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
            is_active: false,
            tab_title: "Terminal".to_string(),
        }
    }

    /// Tạo panel từ session có sẵn (SSH hoặc local).
    ///
    /// Session đã spawn/connect xong, panel chỉ wrap view. Dùng cho SSH
    /// terminal tab — `session` là `Box<dyn TerminalSession>` từ
    /// `SshSession::connect()`.
    pub fn from_session(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_entity = cx.new(|_| session);
        let view = cx.new(|cx| LocalTerminalView::new(session_entity, window, cx));
        view.read(cx).focus_handle(cx).focus(window, cx);
        Self {
            view,
            tab_panel: None,
            is_active: false,
            tab_title: title.to_string(),
        }
    }

    /// Helper tạo `Entity<Self>` từ session có sẵn.
    pub fn from_session_entity(
        session: Box<dyn TerminalSession>,
        title: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::from_session(session, title, window, cx))
    }

    /// Network stats của session (SSH only — `None` cho local).
    /// Dùng cho StatusBar hiển thị tốc độ network.
    pub fn network_stats(&self, cx: &App) -> Option<oneterm_core::NetStats> {
        self.view.read(cx).session.read(cx).network_stats()
    }

    /// Helper tạo `Entity<Self>` (local session mặc định).
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
        // Màu highlight tab active — lấy từ theme (`table.active.border`).
        let highlight = cx.theme().table_active_border;
        let is_active = self.is_active;

        h_flex()
            .id("tab-title")
            .relative()
            .h_full()
            .w_full()
            .min_w(px(100.))
            .items_center()
            .gap_1()
            // Active tab highlight — đường border top 2px lấy màu từ theme.
            // `Tab` bọc title trong 1 inner h_flex (cao 30px, căn giữa trong
            // tab 32px) + `overflow_hidden`, nên đây là vị trí cao nhất có thể
            // chạm tới từ `title()` (mép trên của inner box, ~1px dưới mép tab).
            // Tràn left/right âm để phủ hết bề ngang; phần thừa bị cắt gọn.
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(-px(20.))
                        .right(-px(20.))
                        .h(px(2.))
                        .bg(highlight),
                )
            })
            // Bù padding phải 12px của Tab inner_h_flex để × sát viền phải.
            .mr(-px(5.))
            // Middle-click trên tab → đóng tab đó (kể cả tab inactive).
            .on_mouse_down(MouseButton::Middle, {
                let tp = tab_panel.clone();
                let pe = panel_entity.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(tp) = tp.as_ref().and_then(|tp| tp.upgrade()) {
                        let panel: Arc<dyn PanelView> = Arc::new(pe.clone());
                        tp.update(cx, |tp, cx| {
                            tp.remove_panel(panel, window, cx);
                        });
                    }
                }
            })
            // Tiêu đề tab — co giãn, cắt bớt bằng ellipsis nếu hẹp.
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(self.tab_title.clone()),
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
                        .child(Icon::new(IconName::Close).xsmall().text_color(theme)),
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

    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
        // `TabPanel` gọi hook này khi tab active đổi → mirror để `title()` dùng.
        if self.is_active != active {
            self.is_active = active;
            cx.notify();
        }

        // Khi tab này thành active → trích SFTP từ session (nếu có)
        // và set vào AppState.active_sftp cho SftpPanel observe.
        // Tab mới active sẽ overwrite — không cần set None khi deactivate.
        if active {
            let sftp = self.view.read(cx).session.read(cx).sftp();
            AppState::global(cx).update(cx, |state, cx| {
                state.active_sftp = sftp;
                cx.notify();
            });
        }
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
