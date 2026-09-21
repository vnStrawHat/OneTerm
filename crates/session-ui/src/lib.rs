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

/// `WorkspaceCommands::saved_ssh_sessions` — the saved sessions grouped for the
/// terminal feature's "+" (New Terminal) menu (`IN-0033`): the ungrouped ones
/// first, then one section per group, all in store order.
///
/// `oneterm-terminal-view` cannot depend on this crate (that edge is a cycle:
/// this crate already depends on it), so the list crosses through the command
/// registry in `oneterm-state`.
///
/// Startup invariant: [`SshSessionStore::global`] panics when this crate's
/// [`init`] has not run, so the composition root must call `init` *before* it
/// installs the bundle holding this function pointer. It does
/// (`oneterm_app::init`), which is what makes the panic unreachable rather than
/// merely unlikely — the pointer does not exist until after the store does.
pub fn saved_ssh_sessions(cx: &App) -> oneterm_state::commands::SavedSshSessionSections {
    tree_builder::menu_entries(SshSessionStore::global(cx).read(cx).sessions())
}

/// `WorkspaceCommands::open_new_saved_session_dialog` — open the full "New SSH
/// Session" dialog, the same one the session tree's context menu opens.
///
/// The terminal feature's "+" menu reaches it through the command registry for
/// the same reason `saved_ssh_sessions` does: the reverse crate edge is a cycle
/// (`US-0114`).
pub fn open_new_saved_session_dialog(window: &mut Window, cx: &mut App) {
    if elevated_window_offers_no_ssh("new_ssh_session") {
        return;
    }
    session_dialog::open_session_dialog(window, cx, None, false);
}

/// M1 (`DEC-0019`): an elevated window has no SSH surface at all.
///
/// The guard sits on this crate's **public entry points** — the ones
/// `WorkspaceCommands` holds pointers to — rather than on each caller, so a menu
/// row, a key binding and any future caller are all covered by the same `if`
/// (`IN-0043` MAJ-3).
fn elevated_window_offers_no_ssh(action_id: &str) -> bool {
    let denied = !oneterm_actions::action_allowed_when_elevated(action_id);
    if denied {
        log::info!("elevated window: refusing to open an SSH surface ({action_id})");
    }
    denied
}

/// `WorkspaceCommands::open_saved_ssh_session` — open the connect dialog for a
/// saved session, exactly as `SessionPanel` does.
///
/// Takes the stable id rather than a position, so a session deleted or moved
/// since the caller read the list cannot be mistaken for another one; an id
/// that is gone simply opens nothing.
pub fn open_saved_ssh_session(id: u64, window: &mut Window, cx: &mut App) {
    if elevated_window_offers_no_ssh("open_session") {
        return;
    }
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
