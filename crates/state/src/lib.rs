//! OneTerm cross-cutting global application state.
//!
//! [`AppState`] is the shared bus every feature crate and the shell read/write
//! (active SFTP backend, cwd source, dock area, zoom state). It lives *below*
//! both the shell and the feature crates, which is what keeps the dependency
//! graph acyclic. [`notif_ext`] provides theme-tinted notification builders.

pub mod active_terminal;
pub mod agent_focus;
mod agent_model;
pub mod agent_registry;
pub mod app_state;
pub mod commands;
pub mod dock_persistence;
pub mod dock_util;
pub mod notif_ext;
pub mod paths;
pub mod services;

pub use agent_registry::{
    AgentCard, AgentRegistry, AgentStateCounts, ApprovalInfo, FileEntry, Grouping, Lifecycle,
    ModelInfo, ToolRun,
};
pub use app_state::AppState;
pub use services::AppServices;
