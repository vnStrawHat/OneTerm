//! Domain model & business logic for OneTerm.
//!
//! Leaf crate: không phụ thuộc `gpui`. Chứa types, traits
//! (`TerminalSession`, `FileTransfer`), `AppError`.

pub mod config;
pub mod error;
pub mod sftp;
pub mod terminal;

pub use config::{LocalShellConfig, ShellKind};
pub use error::AppError;
pub use sftp::{FileEntry, FileStat, SftpBackend};
pub use terminal::{CursorBounds, NetStats, SessionEvent, TerminalInfo, TerminalSession};

/// Result type dùng chung cho crate `core`.
pub type Result<T> = std::result::Result<T, AppError>;
