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
/// for a future Agent Mode that will host a different set of panels. `None`
/// hides the right dock entirely (no panel is hosted).
///
/// Persisted in `ui_config.json` (as the serde name of the variant) and applied
/// at startup by `OneTermWorkspace::new`. The mapping to a registered dock
/// panel name lives in `oneterm_state::panel_names::right_dock_panel_name`,
/// so `core` stays free of any UI/dock knowledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RightDockMode {
    /// Right dock hosts the combined Session + SFTP browser side panel.
    #[default]
    SshClient,
    /// Right dock hosts the Agent Mode panels (placeholder for now).
    Agent,
    /// Hide the right dock entirely. The dock is kept collapsed and no panel
    /// is built for it; switching back to `SshClient`/`Agent` reveals it.
    None,
}

impl RightDockMode {
    /// Whether this mode hides the right dock entirely.
    pub fn is_none(self) -> bool {
        matches!(self, RightDockMode::None)
    }
}
