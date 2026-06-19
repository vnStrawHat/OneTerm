//! Status bar — góc trái: đồng hồ datetime, góc phải: Toggle Right Dock.
//!
//! Clock entity được tạo 1 lần trong `MyTermWorkspace::new` và truyền vào đây,
//! tránh tạo mới mỗi render (sẽ drop timer Task → đồng hồ dừng).

use gpui::{Context, Entity, Styled, Window};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockPlacement},
    status_bar::StatusBar,
};

use crate::components::DateTimeClock;
use crate::layout::MyTermWorkspace;

/// Build `StatusBar` cho `MyTermWorkspace`.
///
/// Clock entity (`clock`) được giữ bởi workspace, chỉ tạo 1 lần để timer
/// 1s fire ổn định — không tạo mới mỗi render.
pub fn build_status_bar(
    dock_area: &Entity<DockArea>,
    clock: Entity<DateTimeClock>,
    _window: &mut Window,
    cx: &mut Context<MyTermWorkspace>,
) -> StatusBar {
    let dock_area = dock_area.clone();

    StatusBar::new()
        // Sync border top color với Dock border (cx.theme().border)
        .border_color(cx.theme().border)
        .left(clock)
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
                        });
                    }
                }),
        )
}