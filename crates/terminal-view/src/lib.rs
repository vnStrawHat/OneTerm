//! Terminal panel — each tab is one Terminal.

pub(crate) mod agent;
pub(crate) mod box_drawing;
pub(crate) mod completion;
#[cfg(any(test, feature = "terminal-diagnostics"))]
pub(crate) mod diagnostics;
pub(crate) mod element;
pub(crate) mod handlers;
pub(crate) mod highlight;
pub(crate) mod layout;
pub(crate) mod panel;
pub(crate) mod security;
pub(crate) mod settings_panel;
pub(crate) mod space;
pub(crate) mod status;
pub(crate) mod theme;
pub(crate) mod url;
pub(crate) mod view;

pub use agent::agent_focuser;
#[cfg(any(test, feature = "terminal-diagnostics"))]
pub use diagnostics::TerminalRenderDiagnostics;
pub use panel::{PanelSpec, TerminalPanel};
pub use security::terminal_security_policy;
pub use status::{find_in_active_terminal, new_terminal_with_shell_cmd, status_metrics};

use gpui::App;
use gpui_component::dock::register_panel;
use oneterm_state::panel_names;

/// Initialize the terminal feature.
///
/// Registers the terminal + terminal-settings dock panels so saved layouts
/// deserialize. Called by the app aggregator, which also passes
/// [`status_metrics`] and [`agent_focuser`] to `AppServices::install`.
pub fn init(cx: &mut App) {
    register_panel(cx, panel_names::TERMINAL, |dock_area, _, _, window, cx| {
        Box::new(TerminalPanel::open(
            PanelSpec::DefaultShell {
                workspace: Some(dock_area.entity_id()),
            },
            window,
            cx,
        ))
    });
    register_panel(cx, panel_names::TERMINAL_SETTINGS, |_, _, _, window, cx| {
        Box::new(settings_panel::TerminalSettingsPanel::new_entity(
            window, cx,
        ))
    });
}
