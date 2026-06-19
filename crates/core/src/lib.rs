//! Domain model & business logic for myTerm2.
//!
//! Leaf crate: không phụ thuộc `gpui`. Chứa types, traits
//! (`TerminalSession`, `FileTransfer`), `AppError`.

pub mod error;

pub use error::AppError;

/// Result type dùng chung cho crate `core`.
pub type Result<T> = std::result::Result<T, AppError>;
