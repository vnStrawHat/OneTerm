//! Registered dock panel names — the single source of truth.
//!
//! Every dock panel is registered with the gpui-component `PanelRegistry`
//! under a string name (R12: in the owning feature's `init()`), and the
//! feature-agnostic shell builds panels *by name* (R4). Saved layouts also
//! deserialize by name, so **these string values are a persisted contract —
//! never change them**; only add new constants. Retired names are listed
//! below so they are never reused: `agent_panel` (the Agent Panel, withdrawn by
//! `DEC-0022`; an old `docks.json` may still name it).
//!
//! Sits in `state` because it is the lowest crate shared by the shell, every
//! feature crate, and `app` (R10). `core` stays a pure domain crate and knows
//! nothing about panels.

/// Terminal tab panel (`terminal-view`).
pub const TERMINAL: &str = "terminal";
/// SFTP file browser panel (`sftp-ui`).
pub const SFTP: &str = "sftp";
/// SSH session list panel (`session-ui`).
pub const SESSION: &str = "session";
/// Right-dock SSH Client Mode panel: Session + SFTP split (`app`).
pub const SSH_CLIENT: &str = "ssh_client_panel";
