//! Cấu hình terminal (shell cục bộ, …).
//!
//! Thuần logic, không phụ thuộc GPUI.

pub mod shell;

pub use shell::{LocalShellConfig, ShellKind, resolve_shell};
