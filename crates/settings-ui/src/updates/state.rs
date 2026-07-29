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
    NoChanges(String),
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
            UpdateUiStatus::NoChanges(message) => message.clone(),
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
