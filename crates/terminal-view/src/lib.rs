//! Terminal panel — each tab is one Terminal.

pub mod agent;
pub mod box_drawing;
pub mod cell;
#[cfg(any(test, feature = "terminal-diagnostics"))]
pub mod diagnostics;
pub mod element;
pub mod handlers;
pub mod highlight;
pub mod ime;
pub mod layout;
pub mod panel;
pub mod render;
pub mod scrollbar;
pub mod search;
pub mod settings_panel;
pub mod space;
pub mod status;
pub mod theme;
pub mod url;
pub mod view;

#[cfg(any(test, feature = "terminal-diagnostics"))]
pub use diagnostics::TerminalRenderDiagnostics;
pub use panel::TerminalPanel;
pub use settings_panel::TerminalSettingsPanel;
pub use status::{find_in_active_terminal, new_terminal_with_shell_cmd, register_status_metrics};
pub use theme::{TerminalTheme, build_terminal_theme, ensure_minimum_contrast, resolve_cell_color};
pub use view::LocalTerminalView;

use gpui::App;
use gpui_component::dock::register_panel;

/// Initialize the terminal feature.
///
/// Registers the terminal + terminal-settings dock panels (so saved layouts
/// deserialize) and installs the status-bar metrics provider (breadcrumb + net
/// stats read the active terminal through it). Called by the app aggregator.
pub fn init(cx: &mut App) {
    status::register_status_metrics(cx);
    agent::init(cx);
    register_panel(cx, "terminal", |dock_area, _, _, window, cx| {
        Box::new(panel::TerminalPanel::new_entity_in_workspace(
            dock_area.entity_id(),
            window,
            cx,
        ))
    });
    register_panel(cx, "terminal-settings", |_, _, _, window, cx| {
        Box::new(settings_panel::TerminalSettingsPanel::new_entity(
            window, cx,
        ))
    });
}
