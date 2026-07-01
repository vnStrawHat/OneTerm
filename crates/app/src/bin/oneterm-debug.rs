//! Dev binary entry point → `oneterm-debug` (oneterm-debug.exe / oneterm-debug).
//!
//! Separate process for `cargo run` (default-run), keeping the console to view logs.
//! The WINDOWS subsystem applies only in release builds (`not(debug_assertions)`).
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    oneterm_app::run();
}
