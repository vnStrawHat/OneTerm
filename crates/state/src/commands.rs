//! Injectable workspace commands.
//!
//! The shell (`OneTermWorkspace`) dispatches app-level actions but must not depend
//! on the feature crates. Feature crates register these command function pointers
//! at init; the shell's action handlers call them, passing the `Window` they
//! already hold from the render/listener context. This keeps the shell
//! feature-agnostic while preserving the `Window` access panel construction needs.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, Entity, Window};
use gpui_component::dock::{DockArea, PanelView};
use oneterm_core::{SessionDuplicateConfig, ShellKind, SshDuplicateConfig};
use oneterm_terminal::TerminalSession;

/// Receives a freshly authenticated SSH duplicate for destination-aware placement.
pub type SshDuplicateCompletion =
    Rc<dyn Fn(Box<dyn TerminalSession>, String, SessionDuplicateConfig, &mut Window, &mut App)>;

/// The saved SSH sessions as the "+" (New Terminal) menu lists them: sections of
/// `(group name, rows)`, each row `(stable session id, title, hex colour)`. The
/// sessions that have no group come first, in one section whose group name is
/// empty; then one section per group, in the order the groups appear in the store.
///
/// The colour is the session's own tag, as the right dock's tree draws it, with
/// the session feature's default already applied — the producer resolves it so
/// the default is read in one place and this layer needs no knowledge of it
/// (`US-0110`).
///
/// Primitives rather than the session feature's own types, because `crates/state`
/// sits below it and must not name them (`IN-0033`).
pub type SavedSshSessionSections = Vec<(String, Vec<(u64, String, String)>)>;

/// Command function pointers registered by the feature crates.
#[derive(Clone, Copy)]
pub struct WorkspaceCommands {
    /// Construct a terminal panel bound to a specific shell kind.
    pub new_terminal_with_shell: fn(ShellKind, &mut Window, &mut App) -> Arc<dyn PanelView>,
    /// Open the SSH **Quick Connect** dialog (host/port/user/auth, optional
    /// save). Named after the dialog it opens, not after the action that
    /// reaches it (`US-0114`).
    pub open_quick_connect_dialog: fn(&mut Window, &mut App),
    /// Open the full **New SSH Session** dialog — label, colour, group, jump
    /// host, port forwards, logging. The same dialog the session tree opens;
    /// the "+" menu reaches it through this pointer (`US-0114`).
    pub open_new_saved_session_dialog: fn(&mut Window, &mut App),
    /// The saved SSH sessions, grouped as the "+" menu lists them — see
    /// [`SavedSshSessionSections`].
    ///
    /// Read fresh every time a menu listing them opens, so an added or deleted
    /// session needs no observer wiring. The id is the session store's stable
    /// v2 id, not a position: a concurrent delete must not retarget a click.
    pub saved_ssh_sessions: fn(&App) -> SavedSshSessionSections,
    /// Open the connect dialog for the saved SSH session with that stable id.
    /// A session deleted since the list was read is a no-op.
    pub open_saved_ssh_session: fn(u64, &mut Window, &mut App),
    /// Prompt for authentication and duplicate an SSH session at the requested cwd.
    pub open_duplicate_ssh_dialog:
        fn(SshDuplicateConfig, Option<PathBuf>, SshDuplicateCompletion, &mut Window, &mut App),
    /// Open the General Settings window.
    pub open_settings: fn(&mut App),
    /// Open the About dialog from the application menu.
    pub open_about: fn(&mut Window, &mut App),
    /// Toggle the in-terminal search bar on the active terminal panel.
    pub find_in_active_terminal: fn(&Entity<DockArea>, &mut Window, &mut App),
    /// Snapshot + apply key bindings (the settings feature owns the logic).
    pub setup_key_bindings: fn(&mut App),
}

/// Get the workspace commands from the application service bundle.
///
/// Startup invariant: the bundle is installed before the shell runs (see
/// [`crate::AppServices::global`]).
pub fn commands(cx: &App) -> WorkspaceCommands {
    super::AppServices::workspace_commands(cx)
}
