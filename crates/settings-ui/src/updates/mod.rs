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

pub(crate) use actions::{check_now, skip_offered_version, start_auto_check};

/// M2 (`DEC-0019`): the elevated instance never checks, downloads or installs
/// an update, and says so once in the About surface.
///
/// An elevated updater can write `C:\Program Files` and leave files whose owner
/// or ACL the ordinary instance cannot replace, which would silently change the
/// install for the normal window too. Removing the path is cheaper and safer
/// than warning about it.
pub(crate) fn elevated_never_updates() -> bool {
    let elevated = oneterm_core::elevation::is_elevated();
    if elevated {
        log::info!("Updates are checked and installed from the normal OneTerm window.");
    }
    elevated
}
pub(crate) use groups::{ELEVATED_UPDATES_TEXT, group, network_page};
pub(crate) use install::download_and_install_update;
pub(crate) use state::UpdateUiState;
