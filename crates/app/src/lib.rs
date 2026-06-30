//! OneTerm application core.
//!
//! Khởi tạo application, đăng ký UI, mở window chính.
//! Logic dùng chung cho cả hai binary: `oneterm` (release) và `oneterm-debug` (dev).
//!
//! Hai bin chỉ là shim mỏng gọi [`run`]; nhờ đó mỗi bin có file nguồn riêng
//! (tránh warning "file present in multiple build targets").

use oneterm_ui::layout::OneTermWorkspace;

pub mod assets;
pub mod window;

use assets::CustomAssets;

/// Khởi chạy OneTerm: init logging, app, UI rồi mở window chính.
pub fn run() {
    // Khởi tạo logging — đọc RUST_LOG env var, mặc định: info cho app, warn cho deps.
    // VD: RUST_LOG=debug → thấy debug log; RUST_LOG=ssh=trace → trace SSH crate.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,oneterm=debug"),
    )
    .format_timestamp_secs()
    .init();
    log::info!("OneTerm starting up");

    // Windows: SetConsoleCtrlHandler safety net — ignore CTRL_C_EVENT.
    // Với OpenConsole.exe (từ Windows Terminal), \x03 qua PTY được xử lý
    // đúng cách → OneTerm không nhận signal. Handler này là backup
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
        // Khởi tạo UI OneTerm (register_panel x3, theme action handlers).
        oneterm_ui::init(cx);
        // Bind key bindings cho workspace.
        OneTermWorkspace::bind_keys(cx);

        cx.activate(true);

        // Mở window chính.
        crate::window::open_window(cx).detach();
    });
}
