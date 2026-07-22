//! Per-SFTP-backend browser state store.
//!
//! OneTerm has a single `SftpPanel` (in the right dock) shared across all SSH
//! tabs. Without per-tab state, switching tabs resets the SFTP browser's cwd
//! and wipes the transfer queue — even though background transfer tasks keep
//! running. This module gives each SFTP backend its own snapshot of the
//! browser's UI state, keyed by the backend's stable per-session id.
//!
//! The store is a gpui `Global` so it outlives any one `SftpPanel` (e.g. when
//! the right dock is swapped via the mode toggle: SSH Client → Agent → SSH
//! Client creates a *new* `SftpPanel` — the store preserves each backend's
//! cwd + transfers across that swap).
//!
//! Transfer tasks (upload/download) capture the SFTP backend they run on, so
//! they can use the same key and update the store directly — independent of
//! which tab is currently active. The `SftpPanel` renders the active key's
//! snapshot; a running transfer keeps progressing in its own backend's
//! snapshot and reappears when the user switches back to that tab.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, Global};

use oneterm_core::{FileEntry, SftpBackend, SftpSessionId};

use super::types::{PendingAction, SortColumn, SortDir, TransferItem};

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
    pub cwd: PathBuf,
    pub entries: Vec<FileEntry>,
    pub sort: Option<(SortColumn, SortDir)>,
    pub selected: Option<usize>,
    pub error: Option<String>,

    pub transfers: Vec<TransferItem>,
    pub next_transfer_id: usize,

    pub pending_action: Option<PendingAction>,

    pub follow_terminal_cwd: bool,
    pub last_followed_cwd: Option<PathBuf>,

    /// Path input error flag, captured so the error highlight survives a tab
    /// switch. (The input value itself is re-synced from `cwd` in `render`.)
    pub path_error: bool,
}

impl Default for SftpBrowserState {
    fn default() -> Self {
        Self {
            cwd: PathBuf::new(),
            entries: Vec::new(),
            sort: None,
            selected: None,
            error: None,
            transfers: Vec::new(),
            next_transfer_id: 0,
            pending_action: None,
            follow_terminal_cwd: false,
            last_followed_cwd: None,
            path_error: false,
        }
    }
}

/// Global per-backend SFTP browser state store.
pub struct SftpBrowserStore(std::sync::Mutex<HashMap<BackendKey, SftpBrowserState>>);

impl Global for SftpBrowserStore {}

impl SftpBrowserStore {
    /// Get the global store, initializing an empty one if absent.
    pub fn global(cx: &mut App) -> &Self {
        if cx.try_global::<Self>().is_none() {
            cx.set_global(Self(std::sync::Mutex::new(HashMap::new())));
        }
        cx.global::<Self>()
    }

    /// Get the stored snapshot for `key` (cloned), or a default if absent.
    pub fn get_or_default(&self, key: BackendKey) -> SftpBrowserState {
        self.0
            .lock()
            .expect("SftpBrowserStore poisoned")
            .get(&key)
            .cloned()
            .unwrap_or_default()
    }

    /// Save a snapshot for `key` (overwrites any existing entry).
    pub fn save(&self, key: BackendKey, state: SftpBrowserState) {
        self.0
            .lock()
            .expect("SftpBrowserStore poisoned")
            .insert(key, state);
    }

    /// Read+modify the state for `key` inside a closure (no clone).
    pub fn with_mut<R>(&self, key: BackendKey, f: impl FnOnce(&mut SftpBrowserState) -> R) -> R {
        f(self
            .0
            .lock()
            .expect("SftpBrowserStore poisoned")
            .entry(key)
            .or_default())
    }

    // NOTE: per-item transfer updates go through SftpPanel::update_transfer_for,
    // which uses with_mut + mirrors into the active view.
}
