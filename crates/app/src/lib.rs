//! OneTerm application core.
//!
//! Initializes the application, registers the UI, and opens the main window.
//! Shared logic for both binaries: `oneterm` (release) and `oneterm-debug` (dev).
//!
//! The two binaries are thin shims that call [`run`]; this gives each binary its own
//! source file (avoiding the "file present in multiple build targets" warning).

use std::borrow::Cow;

use oneterm_ui::layout::OneTermWorkspace;

pub mod assets;
pub mod window;

use assets::CustomAssets;

/// Launch OneTerm: initialize logging, the app, the UI, then open the main window.
pub fn run() {
    // Initialize logging — reads the RUST_LOG env var, default: info for the app, warn for deps.
    // E.g. RUST_LOG=debug → show debug logs; RUST_LOG=ssh=trace → trace the SSH crate.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,oneterm=debug"),
    )
    // Suppress framework noise: GPUI's a11y subsystem sends a debug log every frame
    // when accessibility is active — keep it at info even if RUST_LOG=debug.
    .filter_module("gpui", log::LevelFilter::Info)
    .format_timestamp_secs()
    .init();
    log::info!("OneTerm starting up");

    // Windows: SetConsoleCtrlHandler safety net — ignore CTRL_C_EVENT.
    // With OpenConsole.exe (from Windows Terminal), \x03 over the PTY is handled
    // correctly, so OneTerm never receives the signal. This handler is a backup
    // for the case where OpenConsole.exe is missing → fallback to system ConPTY.
    #[cfg(windows)]
    unsafe {
        extern "system" fn ignore_handler(ctrl_type: u32) -> windows_sys::Win32::Foundation::BOOL {
            match ctrl_type {
                windows_sys::Win32::System::Console::CTRL_C_EVENT
                | windows_sys::Win32::System::Console::CTRL_BREAK_EVENT => {
                    windows_sys::Win32::Foundation::TRUE
                }
                _ => windows_sys::Win32::Foundation::FALSE,
            }
        }
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
            Some(ignore_handler),
            windows_sys::Win32::Foundation::TRUE,
        );
    }

    let app = gpui_platform::application().with_assets(CustomAssets);

    app.run(move |cx| {
        // Embed Lilex font (terminal default).
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Regular.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Bold.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-Italic.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../fonts/Lilex-BoldItalic.ttf").as_slice()),
            ])
            .expect("Failed to load Lilex fonts");

        // Initialize gpui-component (theme, dock, root, ...).
        gpui_component::init(cx);

        // Set Lilex as the theme's default monospace font (after init registers Theme).
        cx.global_mut::<gpui_component::Theme>().mono_font_family = "Lilex".into();

        // Initialize the OneTerm UI (register_panel x3, theme action handlers).
        oneterm_ui::init(cx);
        // Bind key bindings for the workspace.
        OneTermWorkspace::bind_keys(cx);

        cx.activate(true);

        // Open the main window.
        crate::window::open_window(cx).detach();
    });
}
