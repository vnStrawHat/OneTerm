//! SFTP browser — panel duyệt file từ xa.
//!
//! Module gốc `file_browser.rs` đã được tách thành nhiều file con
//! để tuân thủ rule ~400 dòng/file (xem `docs/agents/structure.md` §2).

mod actions;
mod panel;
mod persistence;
mod render;
mod render_transfer;
mod table_delegate;
mod transfer;
mod types;

pub use panel::SftpPanel;
