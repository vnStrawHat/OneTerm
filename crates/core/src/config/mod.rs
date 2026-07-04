//! Terminal configuration (local shell, …).
//!
//! Pure logic, no GPUI dependency.

pub mod shell;

pub use shell::{LocalShellConfig, ShellKind, config_dir, home_dir, resolve_shell};
