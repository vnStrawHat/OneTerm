//! Right dock mode — which set of panels the right dock displays.
//!
//! Pure domain enum (no GPUI), so it can be shared by the settings crate
//! (persisted in `ui_config.json`) and the actions crate (the
//! `SetRightDockMode` action payload) without a same-layer dependency.

use serde::{Deserialize, Serialize};

/// Which set of panels the right dock displays.
///
/// `SshClient` is the classic OneTerm right dock: a vertical split of the
/// Session panel (SSH host list) and the SFTP browser. `Agent` is reserved
/// for a future Agent Mode that will host a different set of panels.
///
/// Persisted in `ui_config.json` (as the serde name of the variant) and applied
/// at startup by `OneTermWorkspace::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RightDockMode {
    /// Right dock hosts the combined Session + SFTP browser side panel.
    #[default]
    SshClient,
    /// Right dock hosts the Agent Mode panels (placeholder for now).
    Agent,
}

impl RightDockMode {
    /// The registered `PanelRegistry` name of the dock panel this mode shows.
    ///
    /// The panel names (`"ssh_client_panel"`, `"agent_panel"`) are registered by the
    /// `app` crate; this only returns the string, so `core` stays free of any
    /// UI/dock dependency.
    pub fn panel_name(self) -> &'static str {
        match self {
            RightDockMode::SshClient => "ssh_client_panel",
            RightDockMode::Agent => "agent_panel",
        }
    }
}
