//! Entry point của myTerm2.
//!
//! Khởi tạo application, đăng ký UI, mở window chính.

use gpui_component_assets::Assets;
use myterm2_ui::layout::MyTermWorkspace;

mod window;

fn main() {
    // Windows: Install SetConsoleCtrlHandler để ignore CTRL_C_EVENT.
    // Khi send_ctrl_c() dùng GenerateConsoleCtrlEvent, signal gửi đến ALL
    // processes trong console (kể cả myTerm2). Handler này prevent myTerm2
    // exit khi nhận CTRL_C_EVENT — chỉ shell/child process nhận signal.
    #[cfg(windows)]
    unsafe {
        extern "system" fn ignore_ctrl_c(
            ctrl_type: u32,
        ) -> windows_sys::Win32::Foundation::BOOL {
            // Ignore CTRL_C_EVENT and CTRL_BREAK_EVENT — return TRUE
            // to indicate we handled it (prevent default handler = exit).
            match ctrl_type {
                windows_sys::Win32::System::Console::CTRL_C_EVENT
                | windows_sys::Win32::System::Console::CTRL_BREAK_EVENT => {
                    windows_sys::Win32::Foundation::TRUE
                }
                _ => windows_sys::Win32::Foundation::FALSE,
            }
        }
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
            Some(ignore_ctrl_c),
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
