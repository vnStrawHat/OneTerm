//! AppState — OneTerm's global state.
//!
//! Skeleton: no shared state yet. Will later hold the host list,
//! session state, and ui_state (e.g. `invisible_panels`).

use std::sync::{Arc, Mutex};

use gpui::{App, AppContext, Entity, Global, WeakEntity};
use oneterm_core::SftpBackend;
use oneterm_terminal::CwdSource;
use oneterm_ui::dock::DockArea;

/// The application's global state.
#[derive(Default)]
pub struct AppState {
    /// Weak reference to the DockArea — used by the SSH connect dialog
    /// (adds a terminal tab after a successful connection).
    /// Set in `OneTermWorkspace::new` after the DockArea is created.
    pub dock_area: Option<WeakEntity<DockArea>>,
    /// Mirror of the zoom state (name of the fullscreen panel) — shared with the
    /// `on_release` callback in `window.rs` to save on window close.
    pub zoomed_panel: Option<Arc<Mutex<Option<String>>>>,
    /// Mirror of toggle_button_visible — shared with the `on_release` callback.
    pub toggle_button_visible: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// SFTP backend of the active terminal tab.
    /// `None` = the active tab has no SFTP (local shell or an SSH that does not support SFTP).
    /// Set by `TerminalPanel::set_active(true)` — overwrites the old value when the tab changes.
    pub active_sftp: Option<Arc<dyn SftpBackend>>,
    /// Live cwd source of the active terminal tab (OSC 7).
    /// `None` = the active tab does not expose a cwd (local shell, or SSH without OSC 7 yet).
    /// Set by `TerminalPanel::set_active(true)` alongside `active_sftp`.
    /// Used by the SFTP browser's "sync to terminal cwd" button.
    pub active_cwd_source: Option<Arc<dyn CwdSource>>,
    /// Whether the active terminal tab is a local shell (`true`) or SSH (`false`).
    /// Mirrored by `TerminalPanel::set_active(true)`.
    pub active_is_local: bool,
}

/// Global wrapper for `Entity<AppState>`.
pub struct AppStateGlobal(pub Entity<AppState>);

impl Global for AppStateGlobal {}

impl AppState {
    /// Get the global `Entity<AppState>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AppStateGlobal>().0.clone()
    }

    /// Initialize the global AppState.
    pub fn init(cx: &mut App) {
        let state = cx.new(|_| Self::default());
        cx.set_global(AppStateGlobal(state));
    }
}
