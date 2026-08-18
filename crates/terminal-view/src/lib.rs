//! Terminal panel — each tab is one Terminal.

pub(crate) mod agent;
pub(crate) mod box_drawing;
pub(crate) mod completion;
pub(crate) mod element;
pub(crate) mod handlers;
pub(crate) mod highlight;
pub(crate) mod layout;
pub(crate) mod panel;
pub(crate) mod security;
pub(crate) mod space;
pub(crate) mod status;
pub(crate) mod theme;
pub(crate) mod url;
pub(crate) mod view;

pub use panel::{PanelSpec, TerminalPanel};
pub use security::terminal_security_policy;
pub use status::{find_in_active_terminal, new_terminal_with_shell_cmd};

use gpui::App;
use gpui_component::dock::register_panel;
use oneterm_state::panel_names;

/// Initialize the terminal feature.
///
/// Registers the terminal dock panel (so saved layouts deserialize) and
/// installs the status-bar metrics provider (breadcrumb + net
/// stats read the active terminal through it). Called by the app aggregator.
pub fn init(cx: &mut App) {
    status::register_status_metrics(cx);
    agent::init(cx);
    register_panel(cx, panel_names::TERMINAL, |dock_area, _, _, window, cx| {
        Box::new(TerminalPanel::open(
            PanelSpec::DefaultShell {
                workspace: Some(dock_area.entity_id()),
            },
            window,
            cx,
        ))
    });
}
