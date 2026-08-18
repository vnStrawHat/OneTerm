//! Per-SFTP-backend browser state store.
//!
//! OneTerm has a single `SftpPanel` (in the right dock) shared across all SSH
//! tabs. Without per-tab state, switching tabs resets the SFTP browser's cwd
//! and wipes the transfer queue — even though background transfer tasks keep
//! running. This module gives each SFTP backend its own snapshot of the
//! browser's UI state, keyed by the backend's stable per-session id.
//!
//! The store is a gpui `Global`, created once by [`crate::init`], so it
//! outlives any one `SftpPanel` (e.g. when the right dock is swapped via the
//! mode toggle: SSH Client → Agent → SSH Client creates a *new* `SftpPanel` —
//! the store preserves each backend's cwd + transfers across that swap). It is
//! only touched from the UI thread, so a `RefCell` guards it; callers must not
//! re-enter the store from inside a `with_mut` closure.
//!
//! Transfer tasks (upload/download) capture the SFTP backend they run on, so
//! they can use the same key and update the store directly — independent of
//! which tab is currently active. The `SftpPanel` renders the active key's
//! snapshot; a running transfer keeps progressing in its own backend's
//! snapshot and reappears when the user switches back to that tab.

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use gpui::{App, Global};

use oneterm_core::{FileEntry, RemotePath, SftpBackend, SftpSessionId};

use super::browser_view::{BrowserView, TransferQueueView};
use super::types::{SortColumn, SortDir};

/// Mutation gate that prevents periodic snapshots while browser state is idle.
#[derive(Default)]
pub(crate) struct SnapshotGate {
    dirty: bool,
}

impl SnapshotGate {
    /// Record a browser-state mutation.
    pub(crate) fn mark(&mut self) {
        self.dirty = true;
    }

    /// Consume the pending snapshot request.
    pub(crate) fn take(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Clear a pending request after an explicit backend-transition snapshot.
    pub(crate) fn clear(&mut self) {
        self.dirty = false;
    }
}

/// Stable per-SFTP-session identity used as the browser state key.
pub(crate) type BackendKey = SftpSessionId;

/// Compute the [`BackendKey`] for an SFTP backend.
/// `None` for a local shell (no SFTP backend).
pub(crate) fn backend_key(sftp: &Option<Arc<dyn SftpBackend>>) -> Option<BackendKey> {
    sftp.as_ref().map(|backend| backend.session_id())
}

/// Snapshot of the SFTP browser's UI state for one backend.
///
/// Stored under the backend's [`BackendKey`]; restored when the user switches
/// back to that backend's tab. Owned by the store, not the panel — so a running
/// transfer's progress updates land here even while another tab is active.
#[derive(Clone)]
pub(crate) struct SftpBrowserState {
    /// Directory position, selection, and error flags.
    pub browser: BrowserView,
    /// Immutable entries make unchanged snapshots O(1) in directory size.
    pub entries: Arc<[FileEntry]>,
    pub sort: Option<(SortColumn, SortDir)>,
    /// Transfer queue + id counter (source of truth for background tasks).
    pub transfers: TransferQueueView,
    pub follow_terminal_cwd: bool,
    pub last_followed_cwd: Option<RemotePath>,
}

impl Default for SftpBrowserState {
    fn default() -> Self {
        Self {
            browser: BrowserView::default(),
            entries: Arc::from([]),
            sort: None,
            transfers: TransferQueueView::default(),
            follow_terminal_cwd: false,
            last_followed_cwd: None,
        }
    }
}

#[derive(Default)]
struct SftpBrowserStoreData {
    states: HashMap<BackendKey, SftpBrowserState>,
    backends: HashMap<BackendKey, Weak<dyn SftpBackend>>,
}

/// Global per-backend SFTP browser state store.
///
/// Weak backend registrations let the store purge closed or dropped sessions
/// without retaining protocol objects solely for UI history.
#[derive(Default)]
pub(crate) struct SftpBrowserStore(RefCell<SftpBrowserStoreData>);

impl Global for SftpBrowserStore {}

impl SftpBrowserStore {
    /// Install the (empty) global store. Called once from [`crate::init`].
    pub(crate) fn init(cx: &mut App) {
        cx.set_global(Self::default());
    }

    /// The global store. Panics when [`Self::init`] has not run — an
    /// initialization invariant of the SFTP feature.
    pub(crate) fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    fn data(&self) -> RefMut<'_, SftpBrowserStoreData> {
        self.0.borrow_mut()
    }

    /// Register a live backend and ensure its state entry exists.
    pub(crate) fn track_backend(&self, backend: &Arc<dyn SftpBackend>) -> BackendKey {
        let key = backend.session_id();
        let mut data = self.data();
        data.backends.insert(key, Arc::downgrade(backend));
        data.states.entry(key).or_default();
        key
    }

    /// Get the stored snapshot for `key` (cloned), or a default if absent.
    pub(crate) fn get_or_default(&self, key: BackendKey) -> SftpBrowserState {
        self.data().states.get(&key).cloned().unwrap_or_default()
    }

    /// Return the immutable entry snapshot for a tracked backend.
    pub(crate) fn entries(&self, key: BackendKey) -> Arc<[FileEntry]> {
        self.data()
            .states
            .get(&key)
            .map(|state| Arc::clone(&state.entries))
            .unwrap_or_else(|| Arc::from([]))
    }

    /// Save a snapshot for a tracked backend (overwrites any existing entry).
    pub(crate) fn save(&self, key: BackendKey, state: SftpBrowserState) {
        let mut data = self.data();
        if data.backends.contains_key(&key) {
            data.states.insert(key, state);
        }
    }

    /// Read+modify existing state for `key` without recreating purged sessions.
    pub(crate) fn with_mut<R>(
        &self,
        key: BackendKey,
        f: impl FnOnce(&mut SftpBrowserState) -> R,
    ) -> Option<R> {
        let mut data = self.data();
        data.states.get_mut(&key).map(f)
    }

    /// Purge browser snapshots whose backend has closed or been dropped.
    pub(crate) fn purge_closed(&self) -> usize {
        let mut data = self.data();
        let stale: Vec<_> = data
            .backends
            .iter()
            .filter_map(|(key, backend)| {
                let alive = backend.upgrade().is_some_and(|backend| backend.alive());
                (!alive).then_some(*key)
            })
            .collect();
        for key in &stale {
            data.backends.remove(key);
            data.states.remove(key);
        }
        stale.len()
    }

    // NOTE: per-item transfer updates go through SftpPanel::update_transfer_for,
    // which uses with_mut + mirrors into the active view.
}

#[cfg(test)]
mod tests {
    use super::SnapshotGate;

    #[test]
    fn idle_ticks_do_not_request_repeated_snapshots() {
        let mut gate = SnapshotGate::default();
        assert!(!gate.take());
        assert!(!gate.take());

        gate.mark();
        gate.mark();
        assert!(gate.take());
        assert!(!gate.take());
    }

    use super::*;
    use crate::test_backend::FakeSftpBackend;

    #[test]
    fn closed_backend_state_is_purged_and_cannot_be_recreated() {
        let store = SftpBrowserStore::default();
        let backend: Arc<dyn SftpBackend> = Arc::new(FakeSftpBackend::new());
        let key = store.track_backend(&backend);
        assert!(
            store
                .with_mut(key, |state| state.browser.set_cwd(RemotePath::new("/tmp")))
                .is_some()
        );

        backend.close();
        assert_eq!(store.purge_closed(), 1);
        assert!(store.with_mut(key, |_| ()).is_none());
        assert!(store.0.borrow().states.is_empty());
    }
}
