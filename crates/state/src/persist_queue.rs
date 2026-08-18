//! Coalescing background persist queue.
//!
//! Every mutation replaces `pending`; one background worker drains the queue
//! until it is empty. A stale snapshot therefore can never overwrite a newer
//! one, whichever order the executor runs the writes in (PERF-27).

use std::sync::{Arc, Mutex, MutexGuard};

/// Newest-wins snapshot queue plus the "a worker is draining" flag.
pub struct PersistQueue<T> {
    pending: Option<T>,
    saving: bool,
}

impl<T> Default for PersistQueue<T> {
    fn default() -> Self {
        Self {
            pending: None,
            saving: false,
        }
    }
}

impl<T> PersistQueue<T> {
    /// Fresh, empty queue behind the `Arc<Mutex<_>>` the workers share.
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::default()))
    }

    /// Replace the pending snapshot; returns `true` when the caller must start
    /// a drain worker because none is running.
    pub fn enqueue(queue: &Arc<Mutex<Self>>, snapshot: T) -> bool {
        let mut state = lock(queue);
        state.pending = Some(snapshot);
        if state.saving {
            return false;
        }
        state.saving = true;
        true
    }

    /// Write pending snapshots until the queue is empty; a snapshot queued
    /// while one is being written is picked up by the same worker.
    pub fn drain(queue: &Arc<Mutex<Self>>, mut save: impl FnMut(&T)) {
        loop {
            let snapshot = {
                let mut state = lock(queue);
                match state.pending.take() {
                    Some(snapshot) => snapshot,
                    None => {
                        state.saving = false;
                        return;
                    }
                }
            };
            save(&snapshot);
        }
    }
}

/// A poisoned queue only means another writer panicked mid-save; the snapshot
/// itself is still valid, so keep draining rather than losing the pending write.
fn lock<T>(queue: &Arc<Mutex<PersistQueue<T>>>) -> MutexGuard<'_, PersistQueue<T>> {
    queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn enqueue_spawns_a_worker_only_when_none_is_running() {
        let queue = PersistQueue::new();
        assert!(PersistQueue::enqueue(&queue, "a"));
        assert!(!PersistQueue::enqueue(&queue, "b"));
        let state = lock(&queue);
        assert!(state.saving);
        assert_eq!(state.pending, Some("b"));
    }

    #[test]
    fn drain_saves_only_the_latest_snapshot_and_releases_the_worker() {
        let queue = PersistQueue::new();
        PersistQueue::enqueue(&queue, "old");
        PersistQueue::enqueue(&queue, "new");
        let saved = RefCell::new(Vec::new());

        PersistQueue::drain(&queue, |s| saved.borrow_mut().push(*s));

        assert_eq!(saved.into_inner(), vec!["new"]);
        let state = lock(&queue);
        assert!(!state.saving);
        assert!(state.pending.is_none());
    }

    #[test]
    fn drain_picks_up_a_snapshot_queued_during_a_save() {
        let queue = PersistQueue::new();
        PersistQueue::enqueue(&queue, "first");
        let saved = RefCell::new(Vec::new());

        PersistQueue::drain(&queue, |s| {
            if *s == "first" {
                // Simulates a UI edit landing while the first write is on disk.
                assert!(!PersistQueue::enqueue(&queue, "second"));
            }
            saved.borrow_mut().push(*s);
        });

        assert_eq!(saved.into_inner(), vec!["first", "second"]);
        assert!(!lock(&queue).saving);
    }
}
