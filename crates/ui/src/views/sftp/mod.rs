//! SFTP browser — panel duyệt file từ xa.
//!
//! Module gốc `file_browser.rs` đã được tách thành nhiều file con
//! để tuân thủ rule ~400 dòng/file (xem `docs/agents/structure.md` §2).

mod actions;
mod panel;
mod render;
mod render_list;
mod render_transfer;
mod transfer;
mod types;

pub use panel::SftpPanel;
