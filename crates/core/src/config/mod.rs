//! Cấu hình terminal (shell cục bộ, …).
//!
//! Thuần logic, không phụ thuộc GPUI.

pub mod shell;

pub use shell::{resolve_shell, LocalShellConfig, ShellKind};