//! SFTP browser — panel for browsing remote files.
//!
//! The original `file_browser.rs` module was split into several submodules
//! to comply with the ~400 lines/file rule (see `docs/agents/structure.md` §2).

mod actions;
mod panel;
mod persistence;
mod render;
mod render_transfer;
mod table_delegate;
mod transfer;
mod types;

pub use panel::SftpPanel;
