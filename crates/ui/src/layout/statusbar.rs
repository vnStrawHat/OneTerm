//! Status bar — góc trái: đồng hồ datetime, góc phải: Toggle Right Dock.
//!
//! Clock entity được tạo 1 lần trong `OneTermWorkspace::new` và truyền vào đây,
//! tránh tạo mới mỗi render (sẽ drop timer Task → đồng hồ dừng).

use gpui::{Context, Entity, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockEvent, DockPlacement},
    status_bar::StatusBar,
};

use crate::components::{DateTimeClock, NetSpeedIndicator};
use crate::layout::OneTermWorkspace;

/// Build `StatusBar` cho `OneTermWorkspace`.
///
/// Clock entity (`clock`) được giữ bởi workspace, chỉ tạo 1 lần để timer
/// 1s fire ổn định — không tạo mới mỗi render.
pub fn build_status_bar(
    dock_area: &Entity<DockArea>,
    clock: Entity<DateTimeClock>,
    net_speed: Entity<NetSpeedIndicator>,
    _window: &mut Window,
    cx: &mut Context<OneTermWorkspace>,
) -> StatusBar {
    let dock_area = dock_area.clone();

    StatusBar::new()
        // Sync border top color với Dock border (cx.theme().border)
        .border_color(cx.theme().border)
        .left(clock)
        .left(
            // Separator + network speed indicator.
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border)
        )
        .left(net_speed)
        .right(
            Button::new("toggle-right-dock")
                .ghost()
                .xsmall()
                .icon(IconName::PanelRight)
                .tooltip("Toggle Right Dock")
                .on_click({
                    let dock_area = dock_area.clone();
                    move |_, window, cx| {
                        dock_area.update(cx, |area, cx| {
                            area.toggle_dock(DockPlacement::Right, window, cx);
                            // Trigger save — toggle_dock không emit LayoutChanged.
                            cx.emit(DockEvent::LayoutChanged);
                        });
                    }
                }),
        )
}
