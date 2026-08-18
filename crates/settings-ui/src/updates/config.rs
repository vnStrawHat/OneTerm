//! Update preference mutation and background persistence queue.
//!
//! The `UpdateConfig` entity is the in-memory source of truth for
//! `update_config.json`. This module owns the preference fields; the release
//! checker owns the cache fields and merges them back through
//! [`apply_check_cache`] so a check that finishes late never overwrites an
//! edit made while it was running.

use std::sync::{Arc, Mutex};

use gpui::{App, AppContext as _, Entity, Global};
use oneterm_state::PersistQueue;
use oneterm_update::{UpdateCheckCache, UpdateConfig};

pub(super) struct UpdateConfigGlobal(pub Entity<UpdateConfig>);

pub(super) struct UpdateConfigPersistQueueGlobal(pub Arc<Mutex<PersistQueue<UpdateConfig>>>);

impl Global for UpdateConfigGlobal {}
impl Global for UpdateConfigPersistQueueGlobal {}

pub(super) fn init_globals(cx: &mut App) {
    if cx.try_global::<UpdateConfigPersistQueueGlobal>().is_none() {
        cx.set_global(UpdateConfigPersistQueueGlobal(PersistQueue::new()));
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

/// Apply one preference edit to the live entity and queue it for disk.
pub(super) fn update_preference(cx: &mut App, edit: impl FnOnce(&mut UpdateConfig)) {
    entity(cx).update(cx, |config, cx| {
        edit(config);
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
    let queue = cx.global::<UpdateConfigPersistQueueGlobal>().0.clone();
    if PersistQueue::enqueue(&queue, snapshot) {
        cx.background_executor()
            .spawn(async move {
                // Preferences only: the checker persists its own cache fields
                // through a field-level merge, so neither writer can clobber the
                // other.
                PersistQueue::drain(&queue, |config| {
                    if let Err(error) = UpdateConfig::save_preferences(config) {
                        log::warn!("failed to save update_config.json preferences: {error}");
                    }
                });
            })
            .detach();
    }
}
