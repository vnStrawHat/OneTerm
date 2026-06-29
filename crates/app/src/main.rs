//! Entry point của myTerm2.
//!
//! Khởi tạo application, đăng ký UI, mở window chính.
//!
//! Subsystem: WINDOWS (ẩn console) ở release; giữ console ở dev để xem log/println!.
//! - `windows_subsystem` là attribute riêng Windows → phải gate `target_os`.
//! - ConPTY/OpenConsole tự tạo pseudo-console cho shell con, không cần console của process.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use myterm2_ui::layout::MyTermWorkspace;
mod assets;
use assets::CustomAssets;
mod window;

fn main() {
    // Khởi tạo logging — đọc RUST_LOG env var, mặc định: info cho app, warn cho deps.
    // VD: RUST_LOG=debug → thấy debug log; RUST_LOG=ssh=trace → trace SSH crate.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,myterm2=debug"),
    )
    .format_timestamp_secs()
    .init();
    log::info!("myTerm2 starting up");

    // Windows: SetConsoleCtrlHandler safety net — ignore CTRL_C_EVENT.
    // Với OpenConsole.exe (từ Windows Terminal), \x03 qua PTY được xử lý
    // đúng cách → myTerm2 không nhận signal. Handler này là backup
    // trong trường hợp OpenConsole.exe không có → fallback system ConPTY.
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
