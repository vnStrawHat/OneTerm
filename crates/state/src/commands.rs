//! Injectable workspace commands.
//!
//! The shell (`OneTermWorkspace`) dispatches app-level actions but must not depend
//! on the feature crates. Feature crates register these command function pointers
//! at init; the shell's action handlers call them, passing the `Window` they
//! already hold from the render/listener context. This keeps the shell
//! feature-agnostic while preserving the `Window` access panel construction needs.

use std::sync::Arc;

use gpui::{App, Entity, Global, Window};
use gpui_component::dock::{DockArea, PanelView};
use oneterm_core::ShellKind;

/// Command function pointers registered by the feature crates.
#[derive(Clone, Copy)]
pub struct WorkspaceCommands {
    /// Construct a terminal panel bound to a specific shell kind.
    pub new_terminal_with_shell: fn(ShellKind, &mut Window, &mut App) -> Arc<dyn PanelView>,
    /// Open the "New SSH session" quick-connect dialog.
    pub open_new_session_dialog: fn(&mut Window, &mut App),
    /// Open the General Settings window.
    pub open_settings: fn(&mut App),
    /// Toggle the in-terminal search bar on the active terminal panel.
    pub find_in_active_terminal: fn(&Entity<DockArea>, &mut Window, &mut App),
    /// Snapshot + apply key bindings (the settings feature owns the logic).
    pub setup_key_bindings: fn(&mut App),
}

impl Global for WorkspaceCommands {}

/// Register the workspace command function pointers (called from feature init).
/// Duplicate registration is rejected so a stale feature set cannot be hidden.
pub fn set_commands(cx: &mut App, commands: WorkspaceCommands) -> Result<(), &'static str> {
    if cx.try_global::<WorkspaceCommands>().is_some() {
        return Err("workspace commands are already registered");
    }
    cx.set_global(commands);
    Ok(())
}

/// Get the registered workspace commands, if any.
pub fn commands(cx: &App) -> Option<WorkspaceCommands> {
    cx.try_global::<WorkspaceCommands>().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_terminal(_shell: ShellKind, _window: &mut Window, _cx: &mut App) -> Arc<dyn PanelView> {
        unreachable!("test command is registration-only")
    }

    fn with_window(_window: &mut Window, _cx: &mut App) {}
    fn with_app(_cx: &mut App) {}
    fn with_dock(_dock: &Entity<DockArea>, _window: &mut Window, _cx: &mut App) {}

    fn test_commands() -> WorkspaceCommands {
        WorkspaceCommands {
            new_terminal_with_shell: new_terminal,
            open_new_session_dialog: with_window,
            open_settings: with_app,
            find_in_active_terminal: with_dock,
            setup_key_bindings: with_app,
        }
    }

    #[gpui::test]
    fn registration_is_available_and_rejects_duplicates(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert!(commands(cx).is_none());
            set_commands(cx, test_commands()).unwrap();
            assert!(commands(cx).is_some());
            assert_eq!(
                set_commands(cx, test_commands()),
                Err("workspace commands are already registered")
            );
        });
    }
}
