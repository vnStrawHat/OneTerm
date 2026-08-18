//! Update preference mutation and background persistence queue.
//!
//! The `UpdateConfig` entity is the in-memory source of truth for
//! `update_config.json`. This module owns the preference fields; the release
//! checker owns the cache fields and merges them back through
//! [`apply_check_cache`] so a check that finishes late never overwrites an
//! edit made while it was running.

use std::sync::{Arc, Mutex};

use gpui::{App, AppContext as _, Entity, Global};
use oneterm_update::{UpdateChannel, UpdateCheckCache, UpdateConfig};

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
    if cx.try_global::<UpdateConfigPersistQueueGlobal>().is_none() {
        cx.set_global(UpdateConfigPersistQueueGlobal(Arc::new(Mutex::new(
            UpdateConfigPersistQueueState::default(),
        ))));
    }
    if cx.try_global::<UpdateConfigGlobal>().is_none() {
        // Read only on the UI thread; creating or quarantining the document
        // is disk work that goes through the background persist queue (PERF-27).
        let loaded = UpdateConfig::read();
        let entity = cx.new(|_| loaded.config);
        cx.set_global(UpdateConfigGlobal(entity));
        if loaded.needs_document_repair {
            persist_update_config(cx);
        }
    }
}

pub(super) fn entity(cx: &App) -> Entity<UpdateConfig> {
    cx.global::<UpdateConfigGlobal>().0.clone()
}

fn persist_queue(cx: &App) -> Arc<Mutex<UpdateConfigPersistQueueState>> {
    cx.global::<UpdateConfigPersistQueueGlobal>().0.clone()
}

/// Apply one preference edit to the live entity and queue it for disk.
fn update_preference(cx: &mut App, edit: impl FnOnce(&mut UpdateConfig)) {
    entity(cx).update(cx, |config, cx| {
        edit(config);
        cx.notify();
    });
    persist_update_config(cx);
}

pub(super) fn set_auto_check(cx: &mut App, auto_check: bool) {
    update_preference(cx, |config| config.auto_check = auto_check);
}

pub(super) fn set_channel(cx: &mut App, channel: UpdateChannel) {
    update_preference(cx, |config| config.channel = channel);
}

pub(super) fn set_check_interval_hours(cx: &mut App, hours: u64) {
    update_preference(cx, |config| config.check_interval_hours = hours);
}

pub(super) fn set_proxy_url(cx: &mut App, proxy_url: Option<String>) {
    update_preference(cx, |config| config.proxy_url = proxy_url);
}

pub(super) fn set_verify_certificates(cx: &mut App, verify: bool) {
    update_preference(cx, |config| config.verify_certificates = verify);
}

/// Remember `version` as skipped so checks stop offering it, or clear the
/// skip with `None`.
pub(super) fn set_skipped_version(cx: &mut App, version: Option<String>) {
    update_preference(cx, |config| config.skipped_version = version);
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
    if enqueue_update_config_snapshot(&queue, snapshot) {
        spawn_update_config_persist_worker(queue, cx);
    }
}

/// Store the newest snapshot; returns whether a worker must be spawned
/// (none is running yet). A running worker picks the snapshot up itself.
fn enqueue_update_config_snapshot(
    queue: &Arc<Mutex<UpdateConfigPersistQueueState>>,
    snapshot: UpdateConfig,
) -> bool {
    let mut state = lock_update_config_persist_queue(queue);
    state.pending = Some(snapshot);
    if state.saving {
        false
    } else {
        state.saving = true;
        true
    }
}

fn spawn_update_config_persist_worker(queue: Arc<Mutex<UpdateConfigPersistQueueState>>, cx: &App) {
    cx.background_executor()
        .spawn(async move {
            // Preferences only: the checker persists its own cache fields
            // through a field-level merge, so neither writer can clobber the
            // other.
            drain_update_config_persist_queue(&queue, UpdateConfig::save_preferences);
        })
        .detach();
}

/// Save pending snapshots until the queue is empty, newest first; a snapshot
/// queued while one is being written is picked up by the same worker.
fn drain_update_config_persist_queue(
    queue: &Arc<Mutex<UpdateConfigPersistQueueState>>,
    save: impl Fn(&UpdateConfig) -> std::io::Result<()>,
) {
    loop {
        let snapshot = {
            let mut state = lock_update_config_persist_queue(queue);
            match state.pending.take() {
                Some(snapshot) => snapshot,
                None => {
                    state.saving = false;
                    return;
                }
            }
        };

        if let Err(error) = save(&snapshot) {
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
    use std::cell::RefCell;

    use super::*;

    fn snapshot(proxy: &str) -> UpdateConfig {
        UpdateConfig {
            proxy_url: Some(proxy.to_owned()),
            ..Default::default()
        }
    }

    #[test]
    fn enqueue_spawns_a_worker_only_when_none_is_running() {
        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));

        assert!(enqueue_update_config_snapshot(&queue, snapshot("a")));
        assert!(!enqueue_update_config_snapshot(&queue, snapshot("b")));
        let state = lock_update_config_persist_queue(&queue);
        assert!(state.saving);
        assert_eq!(
            state.pending.as_ref().unwrap().proxy_url.as_deref(),
            Some("b")
        );
    }

    #[test]
    fn drain_saves_only_the_latest_snapshot_and_releases_the_worker() {
        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));
        enqueue_update_config_snapshot(&queue, snapshot("old"));
        enqueue_update_config_snapshot(&queue, snapshot("new"));
        let saved = RefCell::new(Vec::new());

        drain_update_config_persist_queue(&queue, |config| {
            saved.borrow_mut().push(config.proxy_url.clone());
            Ok(())
        });

        assert_eq!(saved.into_inner(), vec![Some("new".to_owned())]);
        let state = lock_update_config_persist_queue(&queue);
        assert!(!state.saving);
        assert!(state.pending.is_none());
    }

    #[test]
    fn drain_picks_up_a_snapshot_queued_during_a_save() {
        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));
        enqueue_update_config_snapshot(&queue, snapshot("first"));
        let saved = RefCell::new(Vec::new());

        drain_update_config_persist_queue(&queue, |config| {
            let proxy = config.proxy_url.clone();
            if proxy.as_deref() == Some("first") {
                // Simulates a UI edit landing while the first write is on disk.
                assert!(!enqueue_update_config_snapshot(&queue, snapshot("second")));
            }
            saved.borrow_mut().push(proxy);
            Ok(())
        });

        assert_eq!(
            saved.into_inner(),
            vec![Some("first".to_owned()), Some("second".to_owned())]
        );
        assert!(!lock_update_config_persist_queue(&queue).saving);
    }

    #[test]
    fn drain_continues_after_a_failed_save() {
        let queue = Arc::new(Mutex::new(UpdateConfigPersistQueueState::default()));
        enqueue_update_config_snapshot(&queue, snapshot("only"));

        drain_update_config_persist_queue(&queue, |_| Err(std::io::Error::other("disk full")));

        assert!(!lock_update_config_persist_queue(&queue).saving);
    }
}
