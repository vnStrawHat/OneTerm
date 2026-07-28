//! Update status and preferences for the About settings page.

use std::sync::{Arc, Mutex};

use gpui::{
    App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    ActiveTheme as _, AxisExt as _, Sizable as _, WindowExt as _,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    input::{Input, InputEvent, InputState},
    label::Label,
    notification::NotificationType,
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem},
    switch::Switch,
    v_flex,
};
use oneterm_state::notif_ext::{notify, notify_with_title};
use oneterm_update::{
    InstallOutcome, StagedUpdate, UpdateCandidate, UpdateCheckResult, UpdateConfig, UpdateManager,
    install_staged_update,
};

use super::items_with_separators;

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

pub(crate) struct UpdateUiStateGlobal(pub Entity<UpdateUiState>);
pub(crate) struct UpdateConfigGlobal(pub Entity<UpdateConfig>);

#[derive(Default)]
pub(crate) struct UpdateConfigPersistQueueState {
    pending: Option<UpdateConfig>,
    saving: bool,
}

pub(crate) struct UpdateConfigPersistQueueGlobal(pub Arc<Mutex<UpdateConfigPersistQueueState>>);

impl Global for UpdateUiStateGlobal {}
impl Global for UpdateConfigGlobal {}
impl Global for UpdateConfigPersistQueueGlobal {}

struct UpdateProxyInputState {
    input: Entity<InputState>,
    initial_value: String,
    _subscription: Subscription,
}

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
        cx.global::<UpdateConfigGlobal>().0.clone()
    }

    fn persist_queue(cx: &App) -> Arc<Mutex<UpdateConfigPersistQueueState>> {
        cx.global::<UpdateConfigPersistQueueGlobal>().0.clone()
    }

    pub(crate) fn global(cx: &App) -> Entity<Self> {
        cx.global::<UpdateUiStateGlobal>().0.clone()
    }

    pub(crate) fn init(cx: &mut App) {
        if cx.try_global::<UpdateUiStateGlobal>().is_none() {
            let entity = cx.new(|_| Self::default());
            cx.set_global(UpdateUiStateGlobal(entity));
        }
        if cx.try_global::<UpdateConfigGlobal>().is_none() {
            let entity = cx.new(|_| UpdateConfig::load());
            cx.set_global(UpdateConfigGlobal(entity));
        }
        if cx.try_global::<UpdateConfigPersistQueueGlobal>().is_none() {
            cx.set_global(UpdateConfigPersistQueueGlobal(Arc::new(Mutex::new(
                UpdateConfigPersistQueueState::default(),
            ))));
        }
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

/// Build the About-page update group.
pub(crate) fn group(cx: &App) -> SettingGroup {
    let state = UpdateUiState::global(cx).read(cx).clone();
    let config = UpdateUiState::config(cx).read(cx).clone();
    SettingGroup::new()
        .title("Updates")
        .items(items_with_separators(vec![
            auto_check_item(config),
            status_item(state),
        ]))
}

pub(crate) fn network_group(cx: &App) -> SettingGroup {
    let config = UpdateUiState::config(cx).read(cx).clone();
    SettingGroup::new()
        .title("Network")
        .items(items_with_separators(vec![
            proxy_item(config.clone()),
            certificate_item(config),
        ]))
}

fn auto_check_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Automatic Checks",
        SettingField::render(move |options, _window, _cx| {
            Switch::new("update-auto-check")
                .checked(config.auto_check)
                .with_size(options.size)
                .on_click(|checked: &bool, _window, cx| {
                    set_auto_check(cx, *checked);
                })
                .into_any_element()
        }),
    )
    .description(
        "Check for updates automatically after startup when the configured interval is due.",
    )
}

fn proxy_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Proxy URL",
        SettingField::render(
            move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
                let current = config.proxy_url.clone().unwrap_or_default();
                let key = SharedString::from(format!(
                    "update-proxy-{}-{}-{}",
                    options.page_ix, options.group_ix, options.item_ix
                ));
                let state_entity = window.use_keyed_state(key, cx, |window, cx| {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("https://proxy.example.com:8080")
                            .default_value(current.clone())
                    });

                    let _subscription = cx.subscribe_in(&input, window, {
                        move |state: &mut UpdateProxyInputState,
                              input,
                              event: &InputEvent,
                              _window,
                              cx| {
                            if !matches!(event, InputEvent::Change) {
                                return;
                            }
                            input.update(cx, |input, cx| {
                                let value = input.value().trim().to_owned();
                                if value == state.initial_value {
                                    return;
                                }
                                state.initial_value = value.clone();
                                set_proxy_url(
                                    cx,
                                    if value.is_empty() { None } else { Some(value) },
                                );
                            });
                        }
                    });

                    UpdateProxyInputState {
                        input,
                        initial_value: current.clone(),
                        _subscription,
                    }
                });

                state_entity.update(cx, |state, cx| {
                    if state.initial_value != current {
                        state.initial_value = current.clone();
                        state.input.update(cx, |input, cx| {
                            input.set_value(SharedString::from(current.clone()), window, cx);
                        });
                    }
                });

                let input = Input::new(&state_entity.read(cx).input).with_size(options.size);
                let input = if options.layout.is_horizontal() {
                    input.w_64()
                } else {
                    input.w_full()
                };
                input.into_any_element()
            },
        ),
    )
    .description("Leave empty to use the automatic proxy from the system or environment.")
}

fn certificate_item(config: UpdateConfig) -> SettingItem {
    SettingItem::new(
        "Verify Certificates",
        SettingField::render(move |options, _window, _cx| {
            Switch::new("update-verify-certificates")
                .checked(config.verify_certificates)
                .with_size(options.size)
                .on_click(|checked: &bool, _window, cx| {
                    set_verify_certificates(cx, *checked);
                })
                .into_any_element()
        }),
    )
    .description(
        "Keep TLS certificate verification enabled unless you fully trust the proxy or network.",
    )
}

fn set_auto_check(cx: &mut App, auto_check: bool) {
    UpdateUiState::config(cx).update(cx, |config, cx| {
        config.auto_check = auto_check;
        cx.notify();
    });
    persist_update_config(cx);
}

fn set_proxy_url(cx: &mut App, proxy_url: Option<String>) {
    UpdateUiState::config(cx).update(cx, |config, cx| {
        config.proxy_url = proxy_url;
        cx.notify();
    });
    persist_update_config(cx);
}

fn set_verify_certificates(cx: &mut App, verify: bool) {
    UpdateUiState::config(cx).update(cx, |config, cx| {
        config.verify_certificates = verify;
        cx.notify();
    });
    persist_update_config(cx);
}

fn persist_update_config(cx: &App) {
    let snapshot = UpdateUiState::config(cx).read(cx).clone();
    let queue = UpdateUiState::persist_queue(cx);
    let should_spawn = {
        let mut state = lock_update_config_persist_queue(&queue);
        state.pending = Some(snapshot);
        if state.saving {
            false
        } else {
            state.saving = true;
            true
        }
    };
    if should_spawn {
        spawn_update_config_persist_worker(queue, cx);
    }
}

fn spawn_update_config_persist_worker(queue: Arc<Mutex<UpdateConfigPersistQueueState>>, cx: &App) {
    cx.background_executor()
        .spawn(async move {
            drain_update_config_persist_queue(queue);
        })
        .detach();
}

fn drain_update_config_persist_queue(queue: Arc<Mutex<UpdateConfigPersistQueueState>>) {
    loop {
        let snapshot = {
            let mut state = lock_update_config_persist_queue(&queue);
            match state.pending.take() {
                Some(snapshot) => snapshot,
                None => {
                    state.saving = false;
                    return;
                }
            }
        };

        if let Err(error) = snapshot.save() {
            log::warn!("failed to save update_config.json: {error}");
        }
    }
}

fn lock_update_config_persist_queue(
    queue: &Arc<Mutex<UpdateConfigPersistQueueState>>,
) -> std::sync::MutexGuard<'_, UpdateConfigPersistQueueState> {
    match queue.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("update config persist queue was poisoned; continuing");
            poisoned.into_inner()
        }
    }
}

/// Start a release-build automatic check when preferences say it is due.
pub fn start_auto_check(window: &mut Window, cx: &mut App) {
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

fn status_item(state: UpdateUiState) -> SettingItem {
    SettingItem::new(
        "Update Status",
        SettingField::render(move |_options, _window, cx| {
            v_flex()
                .w_full()
                .gap_1()
                .child(Label::new(state.status_text()).text_color(cx.theme().muted_foreground))
                .into_any_element()
        }),
    )
    .description("Checks the official GitHub Releases feed inferred from the build repository.")
}

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
            .description(
                "Installing this update will close OneTerm and restart it. Any active ".to_owned()
                    + "terminal, SSH, SFTP, or agent work will close.",
            )
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

fn notify_status(state: &UpdateUiState, window: &mut Window, cx: &mut App) {
    match &state.status {
        UpdateUiStatus::Available(version) => window.push_notification(
            notify_with_title(
                NotificationType::Info,
                format!("OneTerm {version} is available."),
                "Update available",
                cx,
            ),
            cx,
        ),

        UpdateUiStatus::Failed(error) => window.push_notification(
            notify(
                NotificationType::Error,
                format!("Update failed: {error}"),
                cx,
            ),
            cx,
        ),
        UpdateUiStatus::RestartScheduled => window.push_notification(
            notify(
                NotificationType::Info,
                "Quit OneTerm to let the update helper complete installation.",
                cx,
            ),
            cx,
        ),
        UpdateUiStatus::Restarted => window.push_notification(
            notify(
                NotificationType::Success,
                "Update installed. Restarting OneTerm.",
                cx,
            ),
            cx,
        ),
        _ => {}
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

    #[test]
    fn update_config_persist_queue_keeps_latest_snapshot() {
        use std::sync::{Arc, Mutex};

        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));

        let mut first = UpdateConfig::default();
        first.proxy_url = Some("https://old.example".to_owned());
        let mut second = UpdateConfig::default();
        second.proxy_url = Some("https://new.example".to_owned());

        {
            let mut state = lock_update_config_persist_queue(&queue);
            state.pending = Some(first);
            state.saving = true;
        }
        {
            let mut state = lock_update_config_persist_queue(&queue);
            state.pending = Some(second.clone());
        }

        let snapshot = {
            let mut state = lock_update_config_persist_queue(&queue);
            state.pending.take().unwrap()
        };

        assert_eq!(snapshot.proxy_url, second.proxy_url);
    }
}
