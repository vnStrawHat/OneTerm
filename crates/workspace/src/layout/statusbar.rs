//! Status bar — left side: datetime clock + breadcrumb + git status;
//! right side: resource indicator (CPU/memory) + toggle Right Dock.
//!
//! The clock, net-speed, breadcrumb, git-status, and resource entities are created once in
//! `OneTermWorkspace::new` and passed in here, avoiding a fresh one each render
//! (which would drop the timer Task → updates stop).

use gpui::{Context, Styled, Window, div, px};
use gpui_component::dock::{DockEvent, DockPlacement};
use gpui_component::{
    ActiveTheme as _, IconName, Sizable,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};

use crate::layout::OneTermWorkspace;

/// Build the `StatusBar` for `OneTermWorkspace`.
///
/// The indicator entities are read from the workspace, which created them once
/// so their timers fire reliably — not recreated each render.
pub fn build_status_bar(
    workspace: &OneTermWorkspace,
    _window: &mut Window,
    cx: &mut Context<OneTermWorkspace>,
) -> StatusBar {
    let dock_area = workspace.dock_area.clone();
    let clock = workspace.clock.clone();
    let net_speed = workspace.net_speed.clone();
    let breadcrumb = workspace.breadcrumb.clone();
    let git_status = workspace.git_status.clone();
    let resource = workspace.resource.clone();

    StatusBar::new()
        // Sync the top border color with the Dock border (cx.theme().border)
        .border_color(cx.theme().border)
        .left(clock)
        .left(
            // Separator + breadcrumb (cwd + foreground process).
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border),
        )
        .left(breadcrumb)
        .left(
            // Separator + git status (branch, dirty marker, ahead/behind).
            div().w(px(1.)).h(px(12.)).bg(cx.theme().border),
        )
        .left(git_status)
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
