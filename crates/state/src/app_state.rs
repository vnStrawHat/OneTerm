//! AppState — OneTerm's process-wide registry and workspace-scoped active state.
//!
//! Durable settings remain process-wide, while active terminal/SFTP context is
//! keyed by DockArea identity so independent windows do not overwrite one another.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui::{App, AppContext, Entity, EntityId, Global, WeakEntity};
use gpui_component::dock::DockArea;
use oneterm_core::SftpBackend;
use oneterm_terminal::CwdSource;

/// Active terminal context belonging to one dock/workspace.
#[derive(Default, Clone)]
pub struct WorkspaceActiveState {
    /// SFTP backend of the active terminal tab, if available.
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
    /// Live cwd source of the active terminal tab, if available.
    pub active_cwd_source: Option<Arc<dyn CwdSource>>,
    /// Whether the active terminal tab is a local shell.
    pub active_is_local: bool,
}

/// The application's global state.
#[derive(Default)]
pub struct AppState {
    /// Weak reference to the primary DockArea — retained for legacy commands that
    /// do not yet receive an explicit workspace context.
    pub dock_area: Option<WeakEntity<DockArea>>,
    /// Workspace used by legacy constructors that cannot yet receive a DockArea key.
    pub primary_workspace_id: Option<EntityId>,
    /// Per-DockArea active terminal state. The DockArea entity id is stable for the
    /// lifetime of a workspace and prevents one window's SFTP context from leaking
    /// into another window.
    pub active_workspaces: HashMap<EntityId, WorkspaceActiveState>,
    /// Mirror of the zoom state (name of the fullscreen panel) — shared with the
    /// `on_release` callback in `window.rs` to save on window close.
    pub zoomed_panel: Option<Arc<Mutex<Option<String>>>>,
    /// Mirror of toggle_button_visible — shared with the `on_release` callback.
    pub toggle_button_visible: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl AppState {
    /// Register a DockArea as a workspace and return its key.
    pub fn register_workspace(&mut self, dock_area: &WeakEntity<DockArea>) -> EntityId {
        let id = dock_area.entity_id();
        self.active_workspaces.entry(id).or_default();
        self.primary_workspace_id.get_or_insert(id);
        id
    }

    /// Read the active terminal context for a workspace.
    pub fn active_workspace(&self, workspace_id: Option<EntityId>) -> WorkspaceActiveState {
        workspace_id
            .and_then(|id| self.active_workspaces.get(&id).cloned())
            .unwrap_or_default()
    }

    /// Update the active terminal context for a workspace.
    pub fn set_active_workspace(
        &mut self,
        workspace_id: Option<EntityId>,
        sftp: Option<Arc<dyn SftpBackend>>,
        cwd_source: Option<Arc<dyn CwdSource>>,
        is_local: bool,
    ) {
        if let Some(id) = workspace_id {
            let active = self.active_workspaces.entry(id).or_default();
            active.active_sftp = sftp;
            active.active_cwd_source = cwd_source;
            active.active_is_local = is_local;
        }
    }
}

/// Global wrapper for `Entity<AppState>`.
pub struct AppStateGlobal(pub Entity<AppState>);

impl Global for AppStateGlobal {}

impl AppState {
    /// Get the global `Entity<AppState>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AppStateGlobal>().0.clone()
    }

    /// Return the legacy primary workspace id when the registry is initialized.
    pub fn primary_workspace_id(cx: &App) -> Option<EntityId> {
        let global = cx.try_global::<AppStateGlobal>()?;
        global.0.read(cx).primary_workspace_id
    }

    /// Initialize the global AppState once, preserving existing workspaces.
    pub fn init(cx: &mut App) {
        if cx.try_global::<AppStateGlobal>().is_some() {
            return;
        }
        let state = cx.new(|_| Self::default());
        cx.set_global(AppStateGlobal(state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn active_terminal_state_isolated_by_workspace_id(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            AppState::init(cx);
            let first = cx.new(|_| ()).entity_id();
            let second = cx.new(|_| ()).entity_id();
            AppState::global(cx).update(cx, |state, _| {
                state.set_active_workspace(Some(first), None, None, false);
                state.set_active_workspace(Some(second), None, None, true);
            });

            AppState::init(cx);
            let state = AppState::global(cx);
            assert!(!state.read(cx).active_workspace(Some(first)).active_is_local);
            assert!(
                state
                    .read(cx)
                    .active_workspace(Some(second))
                    .active_is_local
            );
        });
    }
}
