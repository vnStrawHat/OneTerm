//! Domain model & business logic for OneTerm.
//!
//! Leaf crate: does not depend on `gpui` or `alacritty_terminal`. Contains the
//! error type, local-shell configuration, and the `SftpBackend` file-transfer
//! trait. The terminal engine (`TerminalSession` + alacritty coupling) lives in
//! the separate `oneterm-terminal` crate.

pub mod config;
pub mod error;
pub mod sftp;
pub mod ssh_config;

pub use config::{LocalShellConfig, RightDockMode, ShellKind, config_dir, home_dir};
pub use error::AppError;
pub use sftp::{FileEntry, FileStat, SftpBackend};
pub use ssh_config::{SshAuthMethod, SshConfig};

/// Shared result type for the `core` crate.
pub type Result<T> = std::result::Result<T, AppError>;
