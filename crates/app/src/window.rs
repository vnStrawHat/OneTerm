//! Open the main window and attach the Root.
//!
//! Mirrors `reference/.../story/examples/dock.rs` `StoryWorkspace::new_local`.

use gpui::{
    App, AppContext, Bounds, Size, Task, WindowBounds, WindowHandle, WindowKind, WindowOptions, px,
    size,
};
use gpui_component::Root;
#[cfg(not(target_os = "linux"))]
use gpui_component::TitleBar;

use oneterm_workspace::{OneTermWorkspace, save_dock_state_on_close};

/// Open the main window and return its task handle.
pub fn open_window(cx: &mut App) -> Task<anyhow::Result<WindowHandle<Root>>> {
    let mut window_size = size(px(1600.0), px(1000.0));
    if let Some(display) = cx.primary_display() {
        let display_size = display.bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.85);
        window_size.height = window_size.height.min(display_size.height * 0.85);
    }

    let window_bounds = Bounds::centered(None, window_size, cx);

    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            #[cfg(not(target_os = "linux"))]
            titlebar: Some(TitleBar::title_bar_options()),
            window_min_size: Some(Size {
                width: px(640.),
                height: px(480.),
            }),
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            kind: WindowKind::Normal,
            ..Default::default()
        };

        let window = cx.open_window(options, |window, cx| {
            let workspace = cx.new(|cx| OneTermWorkspace::new(window, cx));
            cx.new(|cx| Root::new(workspace, window, cx))
        })?;

        window
            .update(cx, |_, window, cx| {
                window.activate_window();
                window.set_window_title("OneTerm");
                cx.on_release(|_, cx| {
                    save_dock_state_on_close(cx);
                    cx.quit();
                })
                .detach();
            })
            .expect("failed to update window");

        Ok(window)
    })
}
