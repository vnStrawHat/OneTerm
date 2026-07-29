//! Update check actions and check-result state transitions.

use gpui::{App, Window};
use oneterm_update::{UpdateCheckResult, UpdateManager};

use super::{
    notify::notify_status,
    state::{UpdateUiState, UpdateUiStatus},
};

/// Start a release-build automatic check when preferences say it is due.
pub(crate) fn start_auto_check(window: &mut Window, cx: &mut App) {
    if cfg!(debug_assertions) && std::env::var_os("ONETERM_UPDATE_AUTO_CHECK_DEBUG").is_none() {
        log::info!("Automatic update check skipped in debug builds.");
        return;
    }
    let state = UpdateUiState::global(cx);
    let config_entity = UpdateUiState::config(cx);
    let config = config_entity.read(cx).clone();
    if !config.auto_check {
        log::info!("Automatic update check is disabled in Settings.");
        return;
    }
    let repository = oneterm_update::UPDATE_REPOSITORY.to_owned();
    state.update(cx, |state, cx| {
        state.status = UpdateUiStatus::Checking;
        cx.notify();
    });
    log::info!("Starting automatic update check for {repository}.");
    window
        .spawn(cx, async move |cx| {
            let (result, next_config) = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = UpdateManager::with_repository(repository, config);
                    let result = manager.check_now();
                    (result, manager.config().clone())
                })
                .await;
            match &result {
                Ok(UpdateCheckResult::Available(candidate)) => {
                    log::info!("Automatic update available: {}", candidate.version);
                }
                Ok(UpdateCheckResult::UpToDate { current_version }) => {
                    log::info!(
                        "Automatic update check completed: {current_version} is up to date."
                    );
                }
                Ok(UpdateCheckResult::NotModified) => {
                    log::info!(
                        "Automatic update check completed: no GitHub release changes detected."
                    );
                }
                Ok(UpdateCheckResult::Disabled(reason)) => {
                    log::info!("Automatic update check skipped: {reason}");
                }
                Err(error) => {
                    log::warn!("Automatic update check failed: {error}");
                }
            }
            let _ = config_entity.update(cx, |config, cx| {
                *config = next_config;
                cx.notify();
            });
            if let Err(error) = cx.update(|window, cx| {
                let mut snapshot = None;
                let _ = state.update(cx, |state, cx| {
                    apply_check_result(state, result);
                    snapshot = Some(state.clone());
                    cx.notify();
                });
                if let Some(snapshot) = snapshot.as_ref() {
                    notify_status(snapshot, window, cx);
                }
            }) {
                log::warn!("failed to update automatic update status: {error:?}");
            }
        })
        .detach();
}

pub(crate) fn check_now(window: &mut Window, cx: &mut App) {
    let state = UpdateUiState::global(cx);
    if state.read(cx).is_busy() {
        return;
    }

    let config_entity = UpdateUiState::config(cx);
    let config = config_entity.read(cx).clone();
    let repository = oneterm_update::UPDATE_REPOSITORY.to_owned();
    state.update(cx, |state, cx| {
        state.status = UpdateUiStatus::Checking;
        cx.notify();
    });

    log::info!("Starting manual update check for {repository}.");
    window
        .spawn(cx, async move |cx| {
            let (result, next_config) = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = UpdateManager::with_repository(repository, config);
                    let result = manager.refresh_now();
                    (result, manager.config().clone())
                })
                .await;

            match &result {
                Ok(UpdateCheckResult::Available(candidate)) => {
                    log::info!("Manual update available: {}", candidate.version);
                }
                Ok(UpdateCheckResult::UpToDate { current_version }) => {
                    log::info!("Manual update check completed: {current_version} is up to date.");
                }
                Ok(UpdateCheckResult::NotModified) => {
                    log::info!(
                        "Manual update check completed: no GitHub release changes detected."
                    );
                }
                Ok(UpdateCheckResult::Disabled(reason)) => {
                    log::info!("Manual update check skipped: {reason}");
                }
                Err(error) => {
                    log::warn!("Manual update check failed: {error}");
                }
            }

            let _ = config_entity.update(cx, |config, cx| {
                *config = next_config;
                cx.notify();
            });
            if let Err(error) = cx.update(|window, cx| {
                let mut snapshot = None;
                let _ = state.update(cx, |state, cx| {
                    apply_check_result(state, result);
                    snapshot = Some(state.clone());
                    cx.notify();
                });
                if let Some(snapshot) = snapshot.as_ref() {
                    notify_status(snapshot, window, cx);
                }
            }) {
                log::warn!("failed to update manual update status: {error:?}");
            }
        })
        .detach();
}

fn apply_check_result(state: &mut UpdateUiState, result: oneterm_core::Result<UpdateCheckResult>) {
    match result {
        Ok(UpdateCheckResult::Available(candidate)) => {
            let candidate = *candidate;
            state.status = UpdateUiStatus::Available(candidate.version.clone());
            state.candidate = Some(candidate);
            state.staged = None;
        }
        Ok(UpdateCheckResult::UpToDate { current_version }) => {
            state.status = UpdateUiStatus::UpToDate(current_version);
            state.candidate = None;
            state.staged = None;
        }
        Ok(UpdateCheckResult::NotModified) => {
            if let Some(candidate) = &state.candidate {
                state.status = UpdateUiStatus::Available(candidate.version.clone());
            } else if let Some(staged) = &state.staged {
                state.status = UpdateUiStatus::Available(staged.version.clone());
            } else {
                state.status =
                    UpdateUiStatus::NoChanges("No GitHub release changes detected.".to_owned());
            }
        }
        Ok(UpdateCheckResult::Disabled(reason)) => state.status = UpdateUiStatus::Disabled(reason),
        Err(error) => state.status = UpdateUiStatus::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use oneterm_update::UpdateCandidate;

    use super::*;

    #[test]
    fn not_modified_result_clears_checking_status() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: None,
            staged: None,
        };

        apply_check_result(&mut state, Ok(UpdateCheckResult::NotModified));

        assert!(!state.is_busy());
        assert!(matches!(
            state.status,
            UpdateUiStatus::NoChanges(message) if message == "No GitHub release changes detected."
        ));
    }

    #[test]
    fn not_modified_result_preserves_existing_candidate() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: Some(UpdateCandidate {
                version: "999.0.0".to_owned(),
                tag_name: "v999.0.0".to_owned(),
                release_name: None,
                release_notes_url: "https://example.invalid/release".to_owned(),
                body: None,
                asset_name: "oneterm-999.0.0-x86_64-pc-windows-msvc.zip".to_owned(),
                asset_url: "https://example.invalid/oneterm.zip".to_owned(),
                asset_digest: format!("sha256:{}", "a".repeat(64)),
                asset_size: None,
                target_triple: "x86_64-pc-windows-msvc".to_owned(),
            }),
            staged: None,
        };

        apply_check_result(&mut state, Ok(UpdateCheckResult::NotModified));

        assert!(!state.is_busy());
        assert!(state.candidate.is_some());
        assert!(matches!(
            state.status,
            UpdateUiStatus::Available(version) if version == "999.0.0"
        ));
    }
}
