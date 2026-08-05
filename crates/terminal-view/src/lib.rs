//! Terminal panel — each tab is one Terminal.

pub(crate) mod agent;
pub(crate) mod box_drawing;
pub(crate) mod cell;
pub(crate) mod completion;
#[cfg(any(test, feature = "terminal-diagnostics"))]
pub(crate) mod diagnostics;
pub(crate) mod element;
pub(crate) mod handlers;
pub(crate) mod highlight;
pub(crate) mod ime;
pub(crate) mod layout;
pub mod panel;
pub(crate) mod render;
pub(crate) mod scroll_handle;
pub(crate) mod search;
pub(crate) mod settings_panel;
pub(crate) mod space;
pub(crate) mod status;
pub(crate) mod theme;
pub(crate) mod url;
pub(crate) mod view;

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
