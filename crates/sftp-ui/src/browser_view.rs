//! The [`SftpPanel`](super::panel::SftpPanel)'s active-view state, grouped by
//! concern so each piece can be mutated through a small set of methods:
//!
//! - [`BrowserView`] — where the browser is (`cwd`), what is selected, and the
//!   listing / path-input error flags.
//! - [`TransferQueueView`] — the transfer items shown under the file list plus
//!   the per-backend transfer-id counter.
//! - [`FollowCwd`] — the "follow terminal cwd" toggle, the last cwd followed,
//!   the live cwd source of the active terminal, and a cache of its last value.
//!
//! Every mutator raises a `dirty` flag; the panel's snapshot timer drains the
//! flags with `take_dirty()` and persists the view into the per-backend store
//! only when something actually changed.

use oneterm_core::RemotePath;
use oneterm_terminal::SharedState;

use super::types::TransferItem;

// ── BrowserView ─────────────────────────────────────────────

/// Directory position, selection, and error flags of the file browser.
#[derive(Clone, Default)]
pub(crate) struct BrowserView {
    /// Remote working directory; empty until the first listing completes.
    cwd: RemotePath,
    /// Selected row index in the table (mirrors `TableEvent::SelectRow` and
    /// context-menu right-click). Used by toolbar and menu actions.
    selected: Option<usize>,
    /// Error of the last directory listing, shown instead of the table.
    error: Option<String>,
    /// The path input holds a path that could not be opened.
    path_error: bool,
    dirty: bool,
}

impl BrowserView {
    pub(crate) fn cwd(&self) -> &RemotePath {
        &self.cwd
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn path_error(&self) -> bool {
        self.path_error
    }

    /// Record a new directory request: `cwd` moves to `path`, the previous
    /// error and selection are cleared.
    pub(crate) fn begin_load(&mut self, path: RemotePath) {
        self.cwd = path;
        self.error = None;
        self.selected = None;
        self.dirty = true;
    }

    /// Replace `cwd` (e.g. with the absolute directory a listing resolved to).
    pub(crate) fn set_cwd(&mut self, cwd: RemotePath) {
        if self.cwd != cwd {
            self.cwd = cwd;
            self.dirty = true;
        }
    }

    pub(crate) fn select(&mut self, index: Option<usize>) {
        if self.selected != index {
            self.selected = index;
            self.dirty = true;
        }
    }

    pub(crate) fn set_error(&mut self, error: Option<String>) {
        if self.error != error {
            self.error = error;
            self.dirty = true;
        }
    }

    pub(crate) fn set_path_error(&mut self, path_error: bool) {
        if self.path_error != path_error {
            self.path_error = path_error;
            self.dirty = true;
        }
    }

    /// Consume the pending-change flag.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

// ── TransferQueueView ───────────────────────────────────────

/// Transfer items of one backend plus its transfer-id counter.
///
/// The per-backend store owns the source of truth; the panel keeps a mirror
/// of the active backend's queue so render can read it without locking.
#[derive(Clone, Default)]
pub(crate) struct TransferQueueView {
    items: Vec<TransferItem>,
    next_id: usize,
    dirty: bool,
}

impl TransferQueueView {
    pub(crate) fn items(&self) -> &[TransferItem] {
        &self.items
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Number of items still `InProgress`.
    pub(crate) fn active_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == super::types::TransferStatus::InProgress)
            .count()
    }

    /// Hand out the next transfer id.
    pub(crate) fn allocate_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id = id.saturating_add(1);
        self.dirty = true;
        id
    }

    /// Keep the id counter ahead of an id allocated elsewhere (the store).
    pub(crate) fn reserve_id(&mut self, id: usize) {
        if self.next_id <= id {
            self.next_id = id.saturating_add(1);
            self.dirty = true;
        }
    }

    pub(crate) fn push(&mut self, item: TransferItem) {
        self.items.push(item);
        self.dirty = true;
    }

    /// Apply `update` to the item with `id`; returns the updated item.
    pub(crate) fn update(
        &mut self,
        id: usize,
        update: impl FnOnce(&mut TransferItem),
    ) -> Option<TransferItem> {
        let item = self.items.iter_mut().find(|item| item.id == id)?;
        update(item);
        self.dirty = true;
        Some(item.clone())
    }

    /// Overwrite the item with `item.id` (mirror of a store update).
    pub(crate) fn replace(&mut self, item: TransferItem) {
        if let Some(slot) = self.items.iter_mut().find(|slot| slot.id == item.id) {
            *slot = item;
            self.dirty = true;
        }
    }

    /// Drop every item that is no longer `InProgress`; returns how many went.
    pub(crate) fn retain_active(&mut self) -> usize {
        let before = self.items.len();
        self.items
            .retain(|item| item.status == super::types::TransferStatus::InProgress);
        let removed = before - self.items.len();
        if removed > 0 {
            self.dirty = true;
        }
        removed
    }

    /// Consume the pending-change flag.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

// ── FollowCwd ───────────────────────────────────────────────

/// "Follow terminal cwd" state: the toggle, the last cwd followed, and the
/// live cwd source of the active terminal tab (OSC 7).
#[derive(Default)]
pub(crate) struct FollowCwd {
    /// When enabled, the browser navigates to the terminal's cwd whenever it
    /// changes. Toggled from the "..." menu checkbox.
    enabled: bool,
    /// The last terminal cwd followed — the polling timer compares against it
    /// so an unchanged cwd never triggers a redundant `read_dir`.
    last: Option<RemotePath>,
    /// Last value observed from `source`. Only used to re-render the toolbar
    /// when OSC 7 arrives after the sync button was drawn disabled; navigation
    /// always reads `source` live.
    cache: Option<RemotePath>,
    /// Live cwd source of the active terminal. `None` = no cwd available.
    source: Option<SharedState>,
    dirty: bool,
}

impl FollowCwd {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn last(&self) -> Option<&RemotePath> {
        self.last.as_ref()
    }

    /// Flip the toggle; returns the new state.
    pub(crate) fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.dirty = true;
        self.enabled
    }

    pub(crate) fn set_last(&mut self, cwd: Option<RemotePath>) {
        if self.last != cwd {
            self.last = cwd;
            self.dirty = true;
        }
    }

    /// Restore the per-backend part (toggle + last followed) from a snapshot.
    pub(crate) fn restore(&mut self, enabled: bool, last: Option<RemotePath>) {
        self.enabled = enabled;
        self.last = last;
    }

    /// The per-backend part (toggle + last followed) for a snapshot.
    pub(crate) fn snapshot(&self) -> (bool, Option<RemotePath>) {
        (self.enabled, self.last.clone())
    }

    /// Track the active terminal's cwd source (changes with the active tab).
    pub(crate) fn set_source(&mut self, source: Option<SharedState>) {
        self.source = source;
    }

    /// The active terminal's current directory, read live from the source.
    ///
    /// The terminal reports its cwd as a host `PathBuf`; for an SSH tab it names
    /// a remote directory, so it is converted to a [`RemotePath`] here.
    pub(crate) fn terminal_cwd(&self) -> Option<RemotePath> {
        self.source
            .as_ref()
            .and_then(|source| source.cwd())
            .map(|cwd| RemotePath::new(cwd.to_string_lossy()))
    }

    /// Refresh the cached terminal cwd; `true` when it changed (re-render).
    pub(crate) fn refresh_cache(&mut self) -> bool {
        let current = self.terminal_cwd();
        if self.cache != current {
            self.cache = current;
            true
        } else {
            false
        }
    }

    /// Forget the terminal association (no SFTP backend).
    pub(crate) fn clear(&mut self) {
        self.enabled = false;
        self.last = None;
        self.cache = None;
        self.dirty = true;
    }

    /// Consume the pending-change flag.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TransferDirection, TransferStatus};

    fn item(id: usize, status: TransferStatus) -> TransferItem {
        TransferItem {
            id,
            direction: TransferDirection::Upload,
            filename: format!("{id}.txt"),
            progress: 0.0,
            status,
            error: None,
        }
    }

    #[test]
    fn browser_view_marks_dirty_only_on_change() {
        let mut view = BrowserView::default();
        assert!(!view.take_dirty());
        view.select(None);
        assert!(!view.take_dirty());
        view.select(Some(2));
        assert!(view.take_dirty());
        view.begin_load(RemotePath::new("/tmp"));
        assert_eq!(view.cwd().as_str(), "/tmp");
        assert_eq!(view.selected(), None);
        assert!(view.take_dirty());
    }

    #[test]
    fn transfer_queue_updates_and_clears_finished_items() {
        let mut queue = TransferQueueView::default();
        assert_eq!(queue.allocate_id(), 0);
        queue.reserve_id(5);
        assert_eq!(queue.allocate_id(), 6);
        queue.push(item(6, TransferStatus::InProgress));
        queue.push(item(7, TransferStatus::Completed));
        assert_eq!(queue.active_count(), 1);
        let updated = queue
            .update(7, |item| item.status = TransferStatus::Error)
            .unwrap();
        assert_eq!(updated.status, TransferStatus::Error);
        assert!(queue.update(42, |_| {}).is_none());
        assert_eq!(queue.retain_active(), 1);
        assert_eq!(queue.items().len(), 1);
        assert_eq!(queue.items()[0].id, 6);
    }

    #[test]
    fn follow_cwd_snapshot_round_trips() {
        let mut follow = FollowCwd::default();
        assert!(follow.toggle());
        follow.set_last(Some(RemotePath::new("/srv")));
        let (enabled, last) = follow.snapshot();
        let mut restored = FollowCwd::default();
        restored.restore(enabled, last);
        assert!(restored.enabled());
        assert_eq!(restored.last(), Some(&RemotePath::new("/srv")));
        assert_eq!(restored.terminal_cwd(), None);
        assert!(!restored.refresh_cache());
    }
}
