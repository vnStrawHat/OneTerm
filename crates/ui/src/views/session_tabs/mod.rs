//! Session tabs — panel displaying the list of sessions.
//!
//! The original `tabs.rs` module was split into several submodules
//! to comply with the ~400 lines/file rule (see `docs/agents/structure.md` §2).

mod connect_dialog;
mod group_combo;
mod panel;
mod rename_group;
mod render;
mod session_dialog;
mod tree_builder;
mod tree_render;

pub use panel::SessionPanel;

pub(crate) use connect_dialog::open_quick_connect_dialog;

