//! Update status and preferences for the About settings page.
//!
//! The update UI is split by responsibility: runtime state, settings groups,
//! preference persistence, check actions, install actions, and notifications.

mod actions;
mod config;
mod groups;
mod install;
mod notify;
mod state;

pub(crate) use actions::{check_now, start_auto_check};
pub(crate) use groups::{group, network_group};
pub(crate) use install::download_and_install_update;
pub(crate) use state::UpdateUiState;
