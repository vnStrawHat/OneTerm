//! Release binary entry point → `oneterm` (oneterm.exe / oneterm).
//!
//! Subsystem: WINDOWS (ẩn console) ở release; giữ console ở dev để xem log/println!.
//! - `windows_subsystem` là attribute riêng Windows → phải gate `target_os`.
//! - ConPTY/OpenConsole tự tạo pseudo-console cho shell con, không cần console của process.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    oneterm_app::run();
}
