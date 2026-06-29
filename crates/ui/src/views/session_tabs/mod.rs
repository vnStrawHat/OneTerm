//! Session tabs — panel hiển thị danh sách session.
//!
//! Module gốc `tabs.rs` đã được tách thành nhiều file con
//! để tuân thủ rule ~400 dòng/file (xem `docs/agents/structure.md` §2).

mod connect_dialog;
mod group_combo;
mod panel;
mod rename_group;
mod render;
mod session_dialog;
mod tree_builder;
mod tree_render;

pub use panel::SessionPanel;
