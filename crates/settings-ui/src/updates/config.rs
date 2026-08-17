//! Update preference mutation and background persistence queue.
//!
//! The `UpdateConfig` entity is the in-memory source of truth for
//! `update_config.json`. This module owns the preference fields; the release
//! checker owns the cache fields and merges them back through
//! [`apply_check_cache`] so a check that finishes late never overwrites an
//! edit made while it was running.

use std::sync::{Arc, Mutex};

use gpui::{App, AppContext as _, Entity, Global};
use oneterm_update::{UpdateCheckCache, UpdateConfig};

pub(super) struct UpdateConfigGlobal(pub Entity<UpdateConfig>);

#[derive(Default)]
pub(super) struct UpdateConfigPersistQueueState {
    pending: Option<UpdateConfig>,
    saving: bool,
}

pub(super) struct UpdateConfigPersistQueueGlobal(pub Arc<Mutex<UpdateConfigPersistQueueState>>);

impl Global for UpdateConfigGlobal {}
impl Global for UpdateConfigPersistQueueGlobal {}

pub(super) fn init_globals(cx: &mut App) {
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

pub(super) fn entity(cx: &App) -> Entity<UpdateConfig> {
    cx.global::<UpdateConfigGlobal>().0.clone()
}

fn persist_queue(cx: &App) -> Arc<Mutex<UpdateConfigPersistQueueState>> {
    cx.global::<UpdateConfigPersistQueueGlobal>().0.clone()
}

pub(super) fn set_auto_check(cx: &mut App, auto_check: bool) {
    entity(cx).update(cx, |config, cx| {
        config.auto_check = auto_check;
        cx.notify();
    });
    persist_update_config(cx);
}

pub(super) fn set_proxy_url(cx: &mut App, proxy_url: Option<String>) {
    entity(cx).update(cx, |config, cx| {
        config.proxy_url = proxy_url;
        cx.notify();
    });
    persist_update_config(cx);
}

pub(super) fn set_verify_certificates(cx: &mut App, verify: bool) {
    entity(cx).update(cx, |config, cx| {
        config.verify_certificates = verify;
        cx.notify();
    });
    persist_update_config(cx);
}

/// Merge cache metadata from a completed check into the live entity.
///
/// Only the checker-owned fields change; preferences edited while the check
/// was running are kept. The checker already persisted the cache fields, so
/// nothing is scheduled for disk here.
pub(super) fn apply_check_cache(cx: &mut App, cache: UpdateCheckCache) {
    entity(cx).update(cx, |config, cx| {
        config.apply_check_cache(cache);
        cx.notify();
    });
}

fn persist_update_config(cx: &App) {
    let snapshot = entity(cx).read(cx).clone();
    let queue = persist_queue(cx);
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

        // Preferences only: the checker persists its own cache fields through
        // a field-level merge, so neither writer can clobber the other.
        if let Err(error) = snapshot.save_preferences() {
            log::warn!("failed to save update_config.json preferences: {error}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_config_persist_queue_keeps_latest_snapshot() {
        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));

        let first = UpdateConfig {
            proxy_url: Some("https://old.example".to_owned()),
            ..Default::default()
        };
        let second = UpdateConfig {
            proxy_url: Some("https://new.example".to_owned()),
            ..Default::default()
        };

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
