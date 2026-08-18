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
//! - is smaller and centered on the active display,
//! - exists at most once: a second request activates the open window (CORR-57).
//!
//! The Linux CSD options (`window_background: Transparent` +
//! `window_decorations: Client`) mirror the main window so the [`Root`]'s
//! `window_border` wrapper draws the client-side decorations.

use anyhow::Context as _;
use gpui::{
    App, AppContext, Bounds, Global, Size, Task, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, px, size,
};
use gpui_component::Root;
#[cfg(not(target_os = "linux"))]
use gpui_component::TitleBar;

use super::panel::SettingsPanel;

/// The single Settings window: either being opened or open (`WindowHandle`).
/// Cleared when a request finds the window closed or the open fails.
enum SettingsWindow {
    Opening,
    Open(WindowHandle<Root>),
}

struct SettingsWindowGlobal(SettingsWindow);

impl Global for SettingsWindowGlobal {}

/// The open Settings window, if it is still alive; forgets a closed one.
fn existing_settings_window(cx: &mut App) -> Option<WindowHandle<Root>> {
    let handle = match cx.try_global::<SettingsWindowGlobal>() {
        Some(SettingsWindowGlobal(SettingsWindow::Open(handle))) => *handle,
        _ => return None,
    };
    if handle.is_active(cx).is_some() {
        return Some(handle);
    }
    cx.remove_global::<SettingsWindowGlobal>();
    None
}

/// Open the Settings window and return its task handle. When the window is
/// already open it is activated instead and the same handle is returned; while
/// a previous request is still opening it, the call fails with an error.
///
/// Detach the returned task if you don't need to await it:
/// `open_settings_window(cx).detach();`
pub fn open_settings_window(cx: &mut App) -> Task<anyhow::Result<WindowHandle<Root>>> {
    if let Some(existing) = existing_settings_window(cx) {
        return Task::ready(
            existing
                .update(cx, |_, window, _| window.activate_window())
                .map(|()| existing),
        );
    }
    if matches!(
        cx.try_global::<SettingsWindowGlobal>(),
        Some(SettingsWindowGlobal(SettingsWindow::Opening))
    ) {
        return Task::ready(Err(anyhow::anyhow!(
            "the Settings window is already being opened"
        )));
    }
    cx.set_global(SettingsWindowGlobal(SettingsWindow::Opening));

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

        let opened = cx.open_window(options, |window, cx| {
            let panel = SettingsPanel::new_entity(window, cx);
            cx.new(|cx| Root::new(panel, window, cx))
        });
        let window = match opened {
            Ok(window) => window,
            Err(error) => {
                _ = cx.update(|cx| {
                    if cx.has_global::<SettingsWindowGlobal>() {
                        cx.remove_global::<SettingsWindowGlobal>();
                    }
                });
                return Err(error);
            }
        };
        cx.update(|cx| cx.set_global(SettingsWindowGlobal(SettingsWindow::Open(window))));

        window
            .update(cx, |_, window, _cx| {
                window.activate_window();
                window.set_window_title("Settings");
            })
            .context("failed to update settings window")?;

        Ok(window)
    })
}
