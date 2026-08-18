//! Registered dock panel names — the single source of truth.
//!
//! Every dock panel is registered with the gpui-component `PanelRegistry`
//! under a string name (R12: in the owning feature's `init()`), and the
//! feature-agnostic shell builds panels *by name* (R4). Saved layouts also
//! deserialize by name, so **these string values are a persisted contract —
//! never change them**; only add new constants.
//!
//! Sits in `state` because it is the lowest crate shared by the shell, every
//! feature crate, and `app` (R10). `core` stays a pure domain crate and knows
//! nothing about panels; the mapping from [`RightDockMode`] to a panel name
//! lives here ([`right_dock_panel_name`]) for the same reason.

use oneterm_core::RightDockMode;

/// Terminal tab panel (`terminal-view`).
pub const TERMINAL: &str = "terminal";
/// SFTP file browser panel (`sftp-ui`).
pub const SFTP: &str = "sftp";
/// SSH session list panel (`session-ui`).
pub const SESSION: &str = "session";
/// Right-dock SSH Client Mode panel: Session + SFTP split (`app`).
pub const SSH_CLIENT: &str = "ssh_client_panel";
/// Right-dock Agent Mode panel (`app`).
pub const AGENT: &str = "agent_panel";

/// The registered panel name the right dock shows for `mode`.
///
/// Returns `None` for [`RightDockMode::None`] (the right dock is hidden and no
/// panel is built).
pub fn right_dock_panel_name(mode: RightDockMode) -> Option<&'static str> {
    match mode {
        RightDockMode::SshClient => Some(SSH_CLIENT),
        RightDockMode::Agent => Some(AGENT),
        RightDockMode::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_dock_mode_maps_to_registered_names() {
        assert_eq!(
            right_dock_panel_name(RightDockMode::SshClient),
            Some(SSH_CLIENT)
        );
        assert_eq!(right_dock_panel_name(RightDockMode::Agent), Some(AGENT));
        assert_eq!(right_dock_panel_name(RightDockMode::None), None);
    }
}
