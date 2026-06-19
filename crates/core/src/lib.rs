//! Domain model & business logic for myTerm2.
//!
//! Leaf crate: không phụ thuộc `gpui`. Chứa types, traits
//! (`TerminalSession`, `FileTransfer`), `AppError`.

pub mod config;
pub mod error;
pub mod terminal;

pub use error::AppError;
pub use config::{LocalShellConfig, ShellKind};
pub use terminal::{CursorBounds, SessionEvent, TerminalSession};

/// Result type dùng chung cho crate `core`.
pub type Result<T> = std::result::Result<T, AppError>;
