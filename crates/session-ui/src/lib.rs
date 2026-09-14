//! Session feature entry point — wires the session panel and dialogs together
//! and exposes the crate's public API.

pub mod session_state;

mod auth_form;
mod common;
mod connect_dialog;
mod forward_rows;
mod group_combo;
mod jump_hops;
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
    JumpChainError, SshAuthPreference, SshSession, SshSessionEntry, SshSessionId, SshSessionStore,
};

use gpui::{App, Window};
use gpui_component::dock::{panel_handle, register_panel};
use oneterm_state::panel_names;

/// `WorkspaceCommands::saved_ssh_sessions` — the saved sessions as
/// `(stable id, display name)` in storage order, for the terminal feature's
/// "+" (New Terminal) menu (`IN-0033`).
///
/// `oneterm-terminal-view` cannot depend on this crate (that edge is a cycle:
/// this crate already depends on it), so the list crosses through the command
/// registry in `oneterm-state`.
pub fn saved_ssh_sessions(cx: &App) -> Vec<(u64, String)> {
    tree_builder::menu_entries(SshSessionStore::global(cx).read(cx).sessions())
}

/// `WorkspaceCommands::open_saved_ssh_session` — open the connect dialog for a
/// saved session, exactly as `SessionPanel` does.
///
/// Takes the stable id rather than a position, so a session deleted or moved
/// since the caller read the list cannot be mistaken for another one; an id
/// that is gone simply opens nothing.
pub fn open_saved_ssh_session(id: u64, window: &mut Window, cx: &mut App) {
    let id = SshSessionId::from_raw(id);
    if let Some(session) = SshSessionStore::global(cx).read(cx).get(id).cloned() {
        connect_dialog::open_connect_dialog(session, id, window, cx);
    }
}

/// Initialize the session feature: initialize the SSH session store global and
/// register the "session" dock panel (so saved layouts deserialize). Called by
/// the app aggregator.
pub fn init(cx: &mut App) {
    session_state::SshSessionStore::init(cx);
    register_panel(cx, panel_names::SESSION, |_, window, cx| {
        panel_handle(panel::SessionPanel::new_entity(window, cx))
    });
}
