//! Open the Settings UI in a **separate window** (rather than a dock panel).
//!
//! The settings window is a standalone `WindowHandle<Root>` that wraps a
//! [`SettingsPanel`] in a gpui-component [`Root`] (required so the `Settings`
//! widget and its inputs have the component context, focus management, etc.).
//!
//! Unlike the main window (`crates/app/src/window.rs`), the settings window:
//! - renders its own in-window title bar, matching the main workspace chrome,
//! - keeps the native window configured for the same desktop title bar behavior
//!   as the main window on non-Linux platforms,
//! - has no `on_release` quit hook, so closing it leaves the app running,
//! - is smaller and centered on the active display.
//!
//! The Linux CSD options (`window_background: Transparent` +
//! `window_decorations: Client`) mirror the main window so the [`Root`]'s
//! `window_border` wrapper draws the client-side decorations.

use gpui::{
    App, AppContext, Bounds, Size, Task, WindowBounds, WindowHandle, WindowKind, WindowOptions, px,
    size,
};
use gpui_component::Root;
#[cfg(not(target_os = "linux"))]
use gpui_component::TitleBar;

use super::SettingsPanel;

/// Open the Settings window and return its task handle.
///
/// Detach the returned task if you don't need to await it:
/// `open_settings_window(cx).detach();`
pub fn open_settings_window(cx: &mut App) -> Task<anyhow::Result<WindowHandle<Root>>> {
    let mut window_size = size(px(950.0), px(700.0));
    if let Some(display) = cx.primary_display() {
        let display_size = display.bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.9);
        window_size.height = window_size.height.min(display_size.height * 0.9);
    }

    let window_bounds = Bounds::centered(None, window_size, cx);

    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(window_bounds)),
            #[cfg(not(target_os = "linux"))]
            titlebar: Some(TitleBar::title_bar_options()),
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            window_min_size: Some(Size {
                width: px(480.),
                height: px(400.),
            }),
            kind: WindowKind::Normal,
            ..Default::default()
        };

        let window = cx.open_window(options, |window, cx| {
            let panel = SettingsPanel::new_entity(window, cx);
            cx.new(|cx| Root::new(panel, window, cx))
        })?;

        window
            .update(cx, |_, window, cx| {
                window.activate_window();
                window.set_window_title("Settings");
                let _ = cx;
            })
            .expect("failed to update settings window");

        Ok(window)
    })
}
