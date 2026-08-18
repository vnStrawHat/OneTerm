//! Session feature entry point — wires the session panel and dialogs together
//! and exposes the crate's public API.

pub mod session_state;

mod auth_form;
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
pub use quick_connect_dialog::{open_duplicate_ssh_dialog, open_quick_connect_dialog};
pub use session_state::{
    SshAuthPreference, SshSession, SshSessionEntry, SshSessionId, SshSessionStore,
};

use gpui::App;
use gpui_component::dock::register_panel;
use oneterm_state::panel_names;

/// Initialize the session feature: initialize the SSH session store global and
/// register the "session" dock panel (so saved layouts deserialize). Called by
/// the app aggregator.
pub fn init(cx: &mut App) {
    session_state::SshSessionStore::init(cx);
    register_panel(cx, panel_names::SESSION, |_, _, _, window, cx| {
        Box::new(panel::SessionPanel::new_entity(window, cx))
    });
}
