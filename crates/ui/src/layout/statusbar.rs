//! Status bar — góc trái: đồng hồ datetime, góc phải: Toggle Right Dock.

use gpui::{Context, Window};
use gpui_component::{
    IconName, Sizable,
    button::{Button, ButtonVariants as _},
    dock::{DockArea, DockPlacement},
    status_bar::StatusBar,
};

use crate::components::DateTimeClock;
use crate::layout::MyTermWorkspace;

/// Build `StatusBar` cho `MyTermWorkspace`.
pub fn build_status_bar(
    dock_area: &gpui::Entity<DockArea>,
    window: &mut Window,
    cx: &mut Context<MyTermWorkspace>,
) -> StatusBar {
    let clock = DateTimeClock::new_entity(window, cx);
    let dock_area = dock_area.clone();

    StatusBar::new().left(clock).right(
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
