//! Session tabs — panel displaying the list of sessions.
//!
//! The original `tabs.rs` module was split into several submodules
//! to comply with the ~400 lines/file rule (see `docs/agents/structure.md` §2).

pub mod session_state;

mod common;
mod connect_dialog;
mod group_combo;
mod panel;
mod quick_connect_dialog;
mod rename_group;
mod render;
mod session_dialog;
mod tree_builder;
mod tree_render;

pub use panel::SessionPanel;
pub use quick_connect_dialog::open_quick_connect_dialog;
pub use session_state::{SshSession, SshSessionStore};

use gpui::App;
use oneterm_ui::dock::register_panel;

/// Initialize the session feature: initialize the SSH session store global and
/// register the "session" dock panel (so saved layouts deserialize). Called by
/// the app aggregator.
pub fn init(cx: &mut App) {
    session_state::SshSessionStore::init(cx);
    register_panel(cx, "session", |_, _, _, window, cx| {
        Box::new(panel::SessionPanel::new_entity(window, cx))
    });
}
