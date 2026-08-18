//! Status bar — left side: datetime clock + network speed + breadcrumb;
//! right side: resource indicator (CPU/memory) + toggle Right Dock.
//!
//! The clock, net-speed, breadcrumb, and resource entities are created once in
//! `OneTermWorkspace::new` and passed in here, avoiding a fresh one each render
//! (which would drop the timer Task → updates stop).

use gpui::{Context, Entity, Styled, Window, div, px};
use gpui_component::dock::{DockArea, DockEvent, DockPlacement};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};

use crate::layout::OneTermWorkspace;
use crate::widgets::StatusText;

/// Build the `StatusBar` for `OneTermWorkspace`.
///
/// The clock entity (`clock`) is held by the workspace and created only once so the
/// 1s timer fires reliably — not recreated each render.
pub fn build_status_bar(
    dock_area: &Entity<DockArea>,
    clock: Entity<StatusText>,
    net_speed: Entity<StatusText>,
    breadcrumb: Entity<StatusText>,
    resource: Entity<StatusText>,
    _window: &mut Window,
    cx: &mut Context<OneTermWorkspace>,
) -> StatusBar {
    let dock_area = dock_area.clone();

    StatusBar::new()
        // Sync the top border color with the Dock border (cx.theme().border)
        .border_color(cx.theme().border)
        .left(clock)
        .left(
            // Separator + breadcrumb (cwd + foreground process).
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border),
        )
        .left(breadcrumb)
        .right(net_speed)
        .right(
            // Separator before the toggle button.
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border),
        )
        .right(resource)
        .right(
            // Separator before the toggle button.
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border),
        )
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
                            // Trigger a save — toggle_dock does not emit LayoutChanged.
                            cx.emit(DockEvent::LayoutChanged);
                        });
                    }
                }),
        )
}
