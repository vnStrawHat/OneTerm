//! Open the main window and attach the Root.
//!
//! Mirrors `reference/.../story/examples/dock.rs` `StoryWorkspace::new_local`.

use anyhow::Context as _;
use gpui::{
    App, AppContext, Bounds, Size, Task, WindowBounds, WindowHandle, WindowKind, WindowOptions, px,
    size,
};
use gpui_component::Root;
#[cfg(not(target_os = "linux"))]
use gpui_component::TitleBar;

use oneterm_settings_ui::start_auto_check;
use oneterm_workspace::OneTermWorkspace;

use crate::crash_report_dialog::show_crash_reports;

/// Open the main window and return its task handle.
pub(crate) fn open_window(
    pending_crash_reports: Vec<crate::crash_report::PendingCrashReport>,
    cx: &mut App,
) -> Task<anyhow::Result<WindowHandle<Root>>> {
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
            .update(cx, |root, window, cx| {
                window.activate_window();
                let elevated = oneterm_core::elevation::is_elevated();
                if !elevated {
                    // M2: an elevated updater can write `C:\Program Files` and
                    // leave files the ordinary instance cannot replace, which
                    // would silently change the install for the normal window
                    // too. So the elevated instance never checks at all.
                    start_auto_check(window, cx);
                    // M7: the reports were still loaded, promoted and pruned —
                    // only the GitHub-draft dialog is suppressed, so an elevated
                    // window is not a route to a browser and a prefilled issue.
                    show_crash_reports(
                        pending_crash_reports,
                        crate::crash_report::delete_pending_report,
                        root,
                        window,
                        cx,
                    );
                }
                // M5: the marker is in the OS title, so the taskbar, Alt-Tab and
                // every screenshot carry it.
                window.set_window_title(oneterm_core::elevation::window_title(elevated));
                // Over-the-shoulder elevation puts this process in another
                // account's profile, so it has none of the user's settings. Say
                // so once; the marker is still correct, because it comes from
                // the token and not from the configuration.
                if elevated && oneterm_settings::UiConfig::global(cx).read(cx).persist_blocked {
                    gpui_component::WindowExt::push_notification(
                        window,
                        oneterm_theme::notif_ext::notify(
                            gpui_component::notification::NotificationType::Info,
                            "Settings could not be read for this account; this window is using the defaults.",
                            cx,
                        ),
                        cx,
                    );
                }
                // Closing the main window quits the app. The workspace persists
                // its final layout synchronously in its own release hook; gpui
                // runs it in the same effect flush (the root drops the workspace
                // right after this listener), before the quit request is
                // processed by the run loop (CORR-04).
                cx.on_release(|_, cx| cx.quit()).detach();
            })
            .context("failed to configure the main window after opening it")?;

        Ok(window)
    })
}
