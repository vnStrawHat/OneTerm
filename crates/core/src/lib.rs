//! Domain model & business logic for OneTerm.
//!
//! Leaf crate: does not depend on `gpui` or `alacritty_terminal`. Contains the
//! error type, local-shell configuration, and the `SftpBackend` file-transfer
//! trait. The terminal engine (`TerminalSession` + alacritty coupling) lives in
//! the separate `oneterm-terminal` crate.

pub mod config;
pub mod error;
pub mod persistence;
pub mod schema;
pub mod session_duplicate;
pub mod sftp;
pub mod ssh_config;

pub use config::{DockPlacement, LocalShellConfig, RightDockMode, ShellKind, config_dir, home_dir};
pub use error::AppError;
pub use persistence::{atomic_write, quarantine_file, update_json_file};
pub use schema::{
    SCHEMA_VERSION_FIELD, migrate_json_value, schema_version, set_schema_version, versioned_object,
};
pub use session_duplicate::{SessionDuplicateConfig, SshDuplicateAuth, SshDuplicateConfig};
pub use sftp::{
    FileEntry, FileStat, RemotePath, SftpBackend, SftpFuture, SftpSessionId, SftpTableState,
    TransferEvent, TransferHandle,
};
pub use ssh_config::{
    ConnectionCancellation, HostKeyPolicy, SecretString, SshAuthMethod, SshConfig,
};

/// Shared result type for the `core` crate.
pub type Result<T> = std::result::Result<T, AppError>;
