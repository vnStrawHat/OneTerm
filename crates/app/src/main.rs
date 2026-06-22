//! Entry point của myTerm2.
//!
//! Khởi tạo application, đăng ký UI, mở window chính.

use gpui_component_assets::Assets;
use myterm2_ui::layout::MyTermWorkspace;

mod window;

fn main() {
    // Windows: Install SetConsoleCtrlHandler để ignore CTRL_C_EVENT.
    // myTerm2 là console app — có thể nhận CTRL_C_EVENT từ ConPTY khi
    // gửi \x03. Handler này prevent myTerm2 exit.
    #[cfg(windows)]
    unsafe {
        // 1. Ignore CTRL+C entirely (process-wide flag).
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
            None,
            windows_sys::Win32::Foundation::TRUE,
        );
        // 2. Also install handler function (belt & suspenders).
        extern "system" fn ignore_handler(
            ctrl_type: u32,
        ) -> windows_sys::Win32::Foundation::BOOL {
            match ctrl_type {
                windows_sys::Win32::System::Console::CTRL_C_EVENT
                | windows_sys::Win32::System::Console::CTRL_BREAK_EVENT
                | windows_sys::Win32::System::Console::CTRL_CLOSE_EVENT => {
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

    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx| {
        // Khởi tạo gpui-component (theme, dock, root, ...).
        gpui_component::init(cx);
        // Khởi tạo UI myTerm2 (register_panel x3, theme action handlers).
        myterm2_ui::init(cx);
        // Bind key bindings cho workspace.
        MyTermWorkspace::bind_keys(cx);

        cx.activate(true);

        // Mở window chính.
        crate::window::open_window(cx).detach();
    });
}