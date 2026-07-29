//! Update download and install actions.

use gpui::{App, Context, Entity, Window};
use gpui_component::{WindowExt as _, button::ButtonVariant, dialog::DialogButtonProps};
use oneterm_update::{InstallOutcome, StagedUpdate, UpdateManager, install_staged_update};

use super::{
    notify::notify_status,
    state::{UpdateUiState, UpdateUiStatus},
};

pub(crate) fn download_and_install_update(window: &mut Window, cx: &mut App) {
    let state = UpdateUiState::global(cx);
    if state.read(cx).is_busy() {
        return;
    }

    let state_for_ok = state.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let state_for_ok = state_for_ok.clone();
        alert
            .confirm()
            .title("Install Update")
            .description("OneTerm restarts and closes active work.")
            .button_props(
                DialogButtonProps::default()
                    .ok_text("Install and Restart")
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text("Cancel")
                    .show_cancel(true),
            )
            .on_ok(move |_, window, cx| {
                start_download_and_install_update(state_for_ok.clone(), window, cx);
                true
            })
    });
}

fn start_download_and_install_update(
    state: Entity<UpdateUiState>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(staged) = state.read(cx).staged.clone() {
        start_install_update(state, staged, window, cx);
        return;
    }

    let config = UpdateUiState::config(cx).read(cx).clone();
    let repository = oneterm_update::UPDATE_REPOSITORY.to_owned();
    let candidate = state.read(cx).candidate.clone();
    let Some(candidate) = candidate else {
        return;
    };
    let version = candidate.version.clone();
    state.update(cx, |state, cx| {
        state.status = UpdateUiStatus::Downloading(version.clone());
        cx.notify();
    });

    window
        .spawn(cx, async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let manager = UpdateManager::with_repository(repository, config);
                    manager.download_and_stage(&candidate)
                })
                .await;

            match result {
                Ok(staged) => {
                    let state_entity = state.clone();
                    let _ = state.update_in(cx, move |state, window, cx| {
                        state.staged = Some(staged.clone());
                        state.status = UpdateUiStatus::Installing;
                        cx.notify();
                        spawn_install_update(state_entity.clone(), staged, window, cx);
                    });
                }
                Err(error) => {
                    let _ = state.update_in(cx, |state, window, cx| {
                        state.status = UpdateUiStatus::Failed(error.to_string());
                        notify_status(state, window, cx);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
}

fn start_install_update(
    state: Entity<UpdateUiState>,
    staged: StagedUpdate,
    window: &mut Window,
    cx: &mut App,
) {
    state.update(cx, |state, cx| {
        state.status = UpdateUiStatus::Installing;
        cx.notify();
    });
    spawn_install_update(state, staged, window, cx);
}

fn spawn_install_update(
    state: Entity<UpdateUiState>,
    staged: StagedUpdate,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move { install_staged_update(&staged) })
                .await;
            let _ = state.update_in(cx, |state, window, cx| {
                apply_install_result(state, result, window, cx);
            });
        })
        .detach();
}

fn apply_install_result(
    state: &mut UpdateUiState,
    result: oneterm_core::Result<InstallOutcome>,
    window: &mut Window,
    cx: &mut Context<UpdateUiState>,
) {
    match result {
        Ok(InstallOutcome::RestartScheduled) => {
            state.status = UpdateUiStatus::RestartScheduled;
            cx.quit();
        }
        Ok(InstallOutcome::Restarted) => {
            state.status = UpdateUiStatus::Restarted;
            cx.quit();
        }
        Ok(InstallOutcome::ManualInstall { package_dir }) => {
            state.status = UpdateUiStatus::Disabled(format!(
                "Install location is not writable. Install manually from {}.",
                package_dir.display()
            ));
        }
        Err(error) => state.status = UpdateUiStatus::Failed(error.to_string()),
    }
    notify_status(state, window, cx);
    cx.notify();
}
