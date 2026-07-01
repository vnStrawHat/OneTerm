//! Domain model & business logic for OneTerm.
//!
//! Leaf crate: does not depend on `gpui`. Contains types and traits
//! (`TerminalSession`, `FileTransfer`), and `AppError`.

pub mod config;
pub mod error;
pub mod sftp;
pub mod terminal;

pub use config::{LocalShellConfig, ShellKind};
pub use error::AppError;
pub use sftp::{FileEntry, FileStat, SftpBackend};
pub use terminal::{CursorBounds, NetStats, SessionEvent, TerminalInfo, TerminalSession};

/// Shared result type for the `core` crate.
pub type Result<T> = std::result::Result<T, AppError>;
