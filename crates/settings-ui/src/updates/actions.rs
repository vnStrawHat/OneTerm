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
    // Honour the configured interval; a manual check bypasses it (ARCH-36).
    if !config.should_auto_check() {
        log::info!(
            "Automatic update check skipped: last check {} is within the {} hour interval.",
            config.last_checked_at.as_deref().unwrap_or("(unknown)"),
            config.effective_check_interval_hours()
        );
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
            let (result, cache) = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = UpdateManager::with_repository(repository, config);
                    let result = manager.check_now();
                    (result, manager.check_cache())
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
                Ok(UpdateCheckResult::Disabled(reason)) => {
                    log::info!("Automatic update check skipped: {reason}");
                }
                Err(error) => {
                    log::warn!("Automatic update check failed: {error}");
                }
            }
            if let Err(error) = cx.update(|window, cx| {
                // Merge only the checker-owned cache fields: preferences edited
                // while the check ran must survive (CORR-15).
                super::config::apply_check_cache(cx, cache);
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
            let (result, cache) = cx
                .background_executor()
                .spawn(async move {
                    let mut manager = UpdateManager::with_repository(repository, config);
                    let result = manager.refresh_now();
                    (result, manager.check_cache())
                })
                .await;

            match &result {
                Ok(UpdateCheckResult::Available(candidate)) => {
                    log::info!("Manual update available: {}", candidate.version);
                }
                Ok(UpdateCheckResult::UpToDate { current_version }) => {
                    log::info!("Manual update check completed: {current_version} is up to date.");
                }
                Ok(UpdateCheckResult::Disabled(reason)) => {
                    log::info!("Manual update check skipped: {reason}");
                }
                Err(error) => {
                    log::warn!("Manual update check failed: {error}");
                }
            }

            if let Err(error) = cx.update(|window, cx| {
                // Merge only the checker-owned cache fields: preferences edited
                // while the check ran must survive (CORR-15).
                super::config::apply_check_cache(cx, cache);
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
        Ok(UpdateCheckResult::Disabled(reason)) => {
            // No installable release for this build: a candidate or staged
            // package from an earlier check must not keep the install button.
            state.status = UpdateUiStatus::Disabled(reason);
            state.candidate = None;
            state.staged = None;
        }
        Err(error) => state.status = UpdateUiStatus::Failed(error.to_string()),
    }
}

/// Skip the version currently offered: persist the preference so later checks
/// stop offering it, and withdraw the offer from the UI (ARCH-36).
pub(crate) fn skip_offered_version(cx: &mut App) {
    let state = UpdateUiState::global(cx);
    let version = {
        let state = state.read(cx);
        if state.is_busy() {
            return;
        }
        let Some(candidate) = state.candidate.as_ref() else {
            return;
        };
        candidate.version.clone()
    };
    super::config::set_skipped_version(cx, Some(version.clone()));
    state.update(cx, |state, cx| {
        apply_skipped_version(state, &version);
        cx.notify();
    });
    log::info!("Update {version} skipped in Settings.");
}

fn apply_skipped_version(state: &mut UpdateUiState, version: &str) {
    if state
        .candidate
        .as_ref()
        .is_some_and(|candidate| candidate.version == version)
    {
        state.candidate = None;
        state.staged = None;
        state.status = UpdateUiStatus::Skipped(version.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(version: &str) -> oneterm_update::UpdateCandidate {
        oneterm_update::UpdateCandidate {
            version: version.to_owned(),
            tag_name: format!("v{version}"),
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

    #[test]
    fn available_result_stores_candidate_and_enables_install() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: None,
            staged: None,
        };

        apply_check_result(
            &mut state,
            Ok(UpdateCheckResult::Available(Box::new(candidate("9.9.9")))),
        );

        assert!(matches!(&state.status, UpdateUiStatus::Available(v) if v == "9.9.9"));
        assert_eq!(
            state.candidate.as_ref().map(|c| c.version.as_str()),
            Some("9.9.9")
        );
        assert!(state.shows_install_button());
        assert!(state.can_install_update());
    }

    #[test]
    fn failed_result_keeps_existing_candidate_but_reports_failure() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: Some(candidate("9.9.9")),
            staged: None,
        };

        apply_check_result(&mut state, Err(oneterm_core::AppError::msg("offline")));

        assert!(matches!(&state.status, UpdateUiStatus::Failed(e) if e.contains("offline")));
        assert!(!state.is_busy());
        assert!(state.candidate.is_some());
    }

    #[test]
    fn skipping_the_offered_version_withdraws_the_offer() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Available("9.9.9".to_owned()),
            candidate: Some(candidate("9.9.9")),
            staged: None,
        };

        apply_skipped_version(&mut state, "1.0.0");
        assert!(state.candidate.is_some());

        apply_skipped_version(&mut state, "9.9.9");
        assert!(state.candidate.is_none());
        assert!(!state.shows_install_button());
        assert!(matches!(&state.status, UpdateUiStatus::Skipped(v) if v == "9.9.9"));
    }

    #[test]
    fn up_to_date_result_clears_checking_status() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: None,
            staged: None,
        };

        apply_check_result(
            &mut state,
            Ok(UpdateCheckResult::UpToDate {
                current_version: "0.3.4".to_owned(),
            }),
        );

        assert!(!state.is_busy());
        assert!(matches!(
            state.status,
            UpdateUiStatus::UpToDate(version) if version == "0.3.4"
        ));
    }

    #[test]
    fn disabled_result_clears_stale_candidate() {
        let mut state = UpdateUiState {
            status: UpdateUiStatus::Checking,
            candidate: Some(candidate("9.9.9")),
            staged: None,
        };

        apply_check_result(
            &mut state,
            Ok(UpdateCheckResult::Disabled("no package".to_owned())),
        );

        assert!(matches!(state.status, UpdateUiStatus::Disabled(_)));
        assert!(state.candidate.is_none());
        assert!(!state.shows_install_button());
    }
}
