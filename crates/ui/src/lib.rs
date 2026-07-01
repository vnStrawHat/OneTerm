//! GPUI views, layout, theme, and state for OneTerm.
//!
//! This crate contains all gpui + gpui-component code. It communicates with
//! `ssh`/`local` through trait abstractions (not yet implemented in this skeleton).

pub mod actions;
pub mod components;
pub mod icon;
pub mod layout;
pub mod state;
pub mod theme;
pub mod views;

use gpui::App;
use gpui_component::dock::register_panel;

use crate::views::{SessionPanel, SftpPanel, TerminalPanel, TerminalSettingsPanel};

/// Initialize the UI: register the 3 leaf panels with the PanelRegistry (for layout deserialization).
///
/// `gpui_component::init(cx)` (called in `app::main`) already initializes the theme,
/// dock, root, etc. and `PanelRegistry::init`. This function only adds OneTerm's 3
/// panels to the registry so `DockArea::load` can restore the previous layout.
pub fn init(cx: &mut App) {
    theme::init(cx);
    state::AppState::init(cx);
    state::TerminalSettings::init(cx);
    state::SshSessionStore::init(cx);

    register_panel(cx, "terminal", |_, _, _, window, cx| {
        Box::new(TerminalPanel::new_entity(window, cx))
    });
    register_panel(cx, "terminal-settings", |_, _, _, window, cx| {
        Box::new(TerminalSettingsPanel::new_entity(window, cx))
    });
    register_panel(cx, "session", |_, _, _, window, cx| {
        Box::new(SessionPanel::new_entity(window, cx))
    });
    register_panel(cx, "sftp", |_, _, _, window, cx| {
        Box::new(SftpPanel::new_entity(window, cx))
    });
}
