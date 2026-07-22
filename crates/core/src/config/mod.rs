//! Terminal configuration (local shell, …).
//!
//! Pure logic, no GPUI dependency.

pub mod dock_mode;
pub mod placement;
pub mod shell;

pub use dock_mode::RightDockMode;
pub use placement::DockPlacement;
pub use shell::{LocalShellConfig, ShellKind, config_dir, home_dir, resolve_shell};
