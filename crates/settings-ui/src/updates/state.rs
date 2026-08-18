//! Runtime update UI state.

use gpui::{App, AppContext as _, Entity, Global};
use oneterm_update::{StagedUpdate, UpdateCandidate, UpdateConfig};

use super::config;

/// Runtime update status mirrored into the settings UI.
#[derive(Clone, Debug)]
pub(crate) enum UpdateUiStatus {
    Idle,
    Checking,
    UpToDate(String),
    Available(String),
    Downloading(String),
    Installing,
    Disabled(String),
    /// The install location is not writable; the staged package must be
    /// installed by hand from this directory.
    ManualInstall(std::path::PathBuf),
    /// The offered version was skipped from Settings/About.
    Skipped(String),
    Failed(String),
    RestartScheduled,
    Restarted,
}

/// Runtime state for update UI controls.
#[derive(Clone, Debug)]
pub(crate) struct UpdateUiState {
    pub status: UpdateUiStatus,
    pub candidate: Option<UpdateCandidate>,
    pub staged: Option<StagedUpdate>,
}

struct UpdateUiStateGlobal(pub Entity<UpdateUiState>);

impl Global for UpdateUiStateGlobal {}

impl Default for UpdateUiState {
    fn default() -> Self {
        Self {
            status: UpdateUiStatus::Idle,
            candidate: None,
            staged: None,
        }
    }
}

impl UpdateUiState {
    pub(crate) fn config(cx: &App) -> Entity<UpdateConfig> {
        config::entity(cx)
    }

    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.global::<UpdateUiStateGlobal>().0.clone()
    }

    pub(crate) fn init(cx: &mut App) {
        if cx.try_global::<UpdateUiStateGlobal>().is_none() {
            let entity = cx.new(|_| Self::default());
            cx.set_global(UpdateUiStateGlobal(entity));
        }
        config::init_globals(cx);
    }

    pub(crate) fn is_busy(&self) -> bool {
        matches!(
            self.status,
            UpdateUiStatus::Checking | UpdateUiStatus::Downloading(_) | UpdateUiStatus::Installing
        )
    }

    pub(crate) fn shows_install_button(&self) -> bool {
        self.candidate.is_some()
            || self.staged.is_some()
            || matches!(
                self.status,
                UpdateUiStatus::Available(_)
                    | UpdateUiStatus::Downloading(_)
                    | UpdateUiStatus::Installing
            )
    }

    pub(crate) fn can_install_update(&self) -> bool {
        (self.candidate.is_some() || self.staged.is_some()) && !self.is_busy()
    }

    /// Whether "Skip This Version" applies: an offer exists and nothing is running.
    pub(crate) fn can_skip_update(&self) -> bool {
        self.candidate.is_some() && self.staged.is_none() && !self.is_busy()
    }

    pub(crate) fn install_button_label(&self) -> &'static str {
        match self.status {
            UpdateUiStatus::Downloading(_) => "Downloading...",
            UpdateUiStatus::Installing => "Installing...",
            _ => "Install Update",
        }
    }

    pub(crate) fn status_text(&self) -> String {
        match &self.status {
            UpdateUiStatus::Idle => format!(
                "Current version: {}. Repository: {}.",
                oneterm_update::CURRENT_VERSION,
                repository_label()
            ),
            UpdateUiStatus::Checking => "Checking GitHub Releases...".to_owned(),
            UpdateUiStatus::UpToDate(version) => format!("OneTerm {version} is up to date."),
            UpdateUiStatus::Available(version) => format!("OneTerm {version} is available."),
            UpdateUiStatus::Downloading(version) => format!("Downloading OneTerm {version}..."),
            UpdateUiStatus::Installing => "Installing update...".to_owned(),
            UpdateUiStatus::Disabled(reason) => reason.clone(),
            UpdateUiStatus::ManualInstall(package_dir) => format!(
                "Install location is not writable. Install manually from {}.",
                package_dir.display()
            ),
            UpdateUiStatus::Skipped(version) => {
                format!(
                    "OneTerm {version} was skipped. Clear it in Settings to be offered it again."
                )
            }
            UpdateUiStatus::Failed(error) => format!("Update failed: {error}"),
            UpdateUiStatus::RestartScheduled => {
                "Update helper started. Quit OneTerm to complete installation.".to_owned()
            }
            UpdateUiStatus::Restarted => {
                "Update installed. A new OneTerm process was started.".to_owned()
            }
        }
    }
}

fn repository_label() -> &'static str {
    if oneterm_update::UPDATE_REPOSITORY.is_empty() {
        "not configured"
    } else {
        oneterm_update::UPDATE_REPOSITORY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> UpdateCandidate {
        UpdateCandidate {
            version: "9.9.9".to_owned(),
            tag_name: "v9.9.9".to_owned(),
            release_name: None,
            release_notes_url: String::new(),
            body: None,
            asset_name: String::new(),
            asset_url: String::new(),
            asset_digest: String::new(),
            asset_size: None,
            target_triple: String::new(),
            prerelease: false,
        }
    }

    fn state(status: UpdateUiStatus, with_candidate: bool) -> UpdateUiState {
        UpdateUiState {
            status,
            candidate: with_candidate.then(candidate),
            staged: None,
        }
    }

    #[test]
    fn idle_and_up_to_date_hide_the_install_button() {
        assert!(!state(UpdateUiStatus::Idle, false).shows_install_button());
        assert!(!state(UpdateUiStatus::UpToDate("1.0.0".into()), false).shows_install_button());
        assert!(!state(UpdateUiStatus::Failed("x".into()), false).shows_install_button());
    }

    #[test]
    fn install_button_is_shown_while_busy_but_disabled() {
        for status in [
            UpdateUiStatus::Downloading("9.9.9".into()),
            UpdateUiStatus::Installing,
        ] {
            let state = state(status, true);
            assert!(state.shows_install_button());
            assert!(state.is_busy());
            assert!(!state.can_install_update());
            assert!(!state.can_skip_update());
        }
    }

    #[test]
    fn available_candidate_enables_install_and_skip() {
        let state = state(UpdateUiStatus::Available("9.9.9".into()), true);
        assert!(state.shows_install_button());
        assert!(state.can_install_update());
        assert!(state.can_skip_update());
        assert_eq!(state.install_button_label(), "Install Update");
    }

    #[test]
    fn checking_without_candidate_is_busy_and_not_installable() {
        let state = state(UpdateUiStatus::Checking, false);
        assert!(state.is_busy());
        assert!(!state.shows_install_button());
        assert!(!state.can_install_update());
    }

    #[test]
    fn status_text_mentions_the_version() {
        assert!(
            state(UpdateUiStatus::Skipped("9.9.9".into()), false)
                .status_text()
                .contains("9.9.9")
        );
        assert!(
            state(UpdateUiStatus::Available("9.9.9".into()), true)
                .status_text()
                .contains("9.9.9")
        );
    }
}
