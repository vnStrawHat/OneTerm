//! Dev binary entry point → `oneterm-debug` (oneterm-debug.exe / oneterm-debug).
//!
//! Process riêng cho `cargo run` (default-run), giữ console để xem log.
//! Subsystem WINDOWS chỉ áp dụng khi build release (`not(debug_assertions)`).
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    oneterm_app::run();
}
