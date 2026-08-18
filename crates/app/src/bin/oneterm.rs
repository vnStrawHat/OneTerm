//! Binary entry point → `oneterm` (oneterm.exe / oneterm).
//!
//! Subsystem: WINDOWS (hides the console) in release; keeps the console in dev to view logs/println!.
//! - `windows_subsystem` is a Windows-only attribute → must be gated on `target_os`.
//! - ConPTY/OpenConsole creates a pseudo-console for the child shell, so the process's
//!   own console is not needed.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    oneterm_app::run();
}
