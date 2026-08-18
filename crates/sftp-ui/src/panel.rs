//! [`SftpPanel`] — leaf panel displaying the SFTP browser.
//!
//! Shows a file tree from a remote SFTP server. One panel for the whole app —
//! observes `AppState.active_sftp` to know which SSH tab is active.
//!
//! The file list is rendered with `gpui_component::table::DataTable`:
//! - Columns are resizable, sortable, and can be shown/hidden (config).
//! - The Name column is pinned left with the largest width (length priority).
//! - Column state (width + visibility) is persisted to `docks.json`
//!   (the `sftp_table_state` field).
//!
//! The panel's mutable state is grouped into [`BrowserView`],
//! [`TransferQueueView`] and [`FollowCwd`] (see [`super::browser_view`]); the
//! other modules of this crate reach them through the accessors below.
//!
//! See `docs/sftp-browser-design.md` §4.

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, IntoElement,
    Subscription, Task, Window,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::table::{TableEvent, TableState};

use oneterm_core::{RemotePath, SftpBackend};
use oneterm_state::AppState;
use oneterm_terminal::CwdSource;

use super::browser_state::{BackendKey, SftpBrowserState, SftpBrowserStore, SnapshotGate};
use super::browser_view::{BrowserView, FollowCwd, TransferQueueView};
use super::persistence::{read_sftp_table_state, write_sftp_table_state};
use super::table_delegate::SftpTableDelegate;

// ── SftpPanel ────────────────────────────────────────────────

/// Panel displaying the SFTP browser.
///
/// `panel_name = "sftp"`. One panel per workspace in the right dock.
/// Observes the active state keyed by its DockArea workspace when the SSH tab changes.
pub struct SftpPanel {
    focus_handle: FocusHandle,

    // ── SFTP backend ────────────────────────────────────────
    sftp: Option<Arc<dyn SftpBackend>>,
    /// Stable store key for the active backend. `None` = no SFTP backend
    /// (local shell). Tracked so the panel knows which store entry
    /// owns the currently-displayed cwd/entries/transfers.
    active_key: Option<BackendKey>,

    // ── Active view (mirrored from the store on tab switch) ─
    browser: BrowserView,
    transfers: TransferQueueView,
    follow: FollowCwd,
    /// Incremented by every directory request. A listing result is applied only
    /// when its captured generation (and backend key) still match, so a slower
    /// earlier request cannot overwrite a newer one.
    load_generation: u64,
    /// Entries + sort + loading + column config live in the delegate.
    table: Entity<TableState<SftpTableDelegate>>,

    // ── Path input (toolbar) ────────────────────────────────
    path_input: Entity<InputState>,
    _path_sub: Subscription,

    /// The poll timer (terminal cwd cache, auto-follow, deferred store
    /// snapshot). Runs only while a backend is active; see [`Self::ensure_poll_timer`].
    _follow_task: Option<Task<()>>,
    /// `true` while the poll timer loop is alive.
    poll_running: bool,
    /// Mutation gate for panel-level changes (column widths, sort) that are
    /// not tracked by one of the views.
    snapshot_gate: SnapshotGate,
    /// Whether the table entry ordering/content changed since its stored Arc snapshot.
    entries_dirty: bool,

    // ── Debounce save table state ───────────────────────────
    _save_table_task: Option<Task<()>>,
    _table_sub: Subscription,
}

impl SftpPanel {
    /// Create a new panel for the primary workspace. Test-only helper; production
    /// builds panels bound to a specific dock via [`Self::new_in_workspace`].
    #[cfg(test)]
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace_id = AppState::primary_workspace_id(cx);
        Self::new_internal(workspace_id, window, cx)
    }

    /// Create a new panel bound to a specific dock/workspace.
    pub(crate) fn new_in_workspace(
        workspace_id: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(Some(workspace_id), window, cx)
    }

    fn new_internal(
        workspace_id: Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();

        let app_state = AppState::global(cx);
        log::debug!("SftpPanel::new: observing workspace active state changes");

        // DataTable state — delegate owns entries + column config (persisted).
        let panel_weak = cx.entity().downgrade();
        let delegate = SftpTableDelegate::new(panel_weak);
        let table = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .col_movable(false)
                .col_resizable(true)
                .sortable(true)
                .col_selectable(false)
                .row_selectable(true)
        });

        // Subscribe table events → mirror selection, handle double-click navigate,
        // persist column widths on resize.
        let table_sub = cx.subscribe_in(&table, window, Self::on_table_event);

        cx.observe(&app_state, move |this, state, cx| {
            let active = state.read(cx).active_workspace(workspace_id);
            let (sftp, cwd_source) = (active.active_sftp, active.active_cwd_source);
            this.sync_from_app_state(sftp, cwd_source, cx);
            cx.notify();
        })
        .detach();

        // Path input — display cwd, Enter → goto path.
        let path_input = cx.new(|cx| InputState::new(window, cx).placeholder("Path"));
        let _path_sub = cx.subscribe_in(&path_input, window, Self::on_path_input_event);

        let mut me = Self {
            focus_handle,
            sftp: None,
            active_key: None,
            browser: BrowserView::default(),
            transfers: TransferQueueView::default(),
            follow: FollowCwd::default(),
            load_generation: 0,
            table,
            path_input,
            _path_sub,
            _follow_task: None,
            poll_running: false,
            snapshot_gate: SnapshotGate::default(),
            entries_dirty: false,
            _save_table_task: None,
            _table_sub: table_sub,
        };

        // Seed from the current AppState so a freshly-created panel (e.g. after
        // switching SSH Client → Agent → SSH Client) shows the active SSH tab's
        // SFTP connection immediately. The observer above only fires on *change*,
        // so without this seed the panel stays empty when the active tab is the
        // same one that was active before the swap.
        let active = app_state.read(cx).active_workspace(workspace_id);
        let (sftp, cwd_source) = (active.active_sftp, active.active_cwd_source);
        me.sync_from_app_state(sftp, cwd_source, cx);
        me.load_table_state(cx);

        me
    }

    /// Start the 500 ms poll timer if it is not already running (CORR-67).
    ///
    /// Each tick refreshes the cached terminal cwd, auto-follows it when the
    /// toggle is on, and snapshots the view into the per-backend store after a
    /// mutation. None of that is needed without an active backend, so the loop
    /// ends itself when the panel shows "No SFTP connection" and is restarted
    /// by the next backend switch.
    fn ensure_poll_timer(&mut self, cx: &mut Context<Self>) {
        if self.poll_running {
            return;
        }
        self.poll_running = true;
        self._follow_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                // The panel may be gone; then there is nothing left to poll.
                let keep_polling = this
                    .update(cx, |this, cx| {
                        if this.active_key.is_none() {
                            this.poll_running = false;
                            return false;
                        }
                        this.refresh_terminal_cwd_cache(cx);
                        this.maybe_follow_terminal_cwd(cx);
                        // Persist only after a browser-state mutation. The timer
                        // remains responsible for saving changes made by a panel
                        // that is removed without a backend transition, but it no
                        // longer clones directory entries while the panel is idle.
                        SftpBrowserStore::global(cx).purge_closed();
                        if this.take_dirty() {
                            if let Some(key) = this.active_key {
                                this.save_state_for_key(key, cx);
                            }
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        }));
    }

    /// `true` while the poll timer loop is alive (test hook for CORR-67).
    #[cfg(test)]
    pub(crate) fn poll_timer_running(&self) -> bool {
        self.poll_running
    }

    /// Read the persisted column state off the UI thread and apply it once it
    /// arrives. Filesystem reads must not run on the UI thread (see
    /// `docs/agents/persistence.md`).
    fn load_table_state(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let state = cx
                .background_executor()
                .spawn(async { read_sftp_table_state() })
                .await;
            let Some(state) = state else {
                return;
            };
            // The panel may be gone before the read completes; nothing to apply then.
            _ = this.update(cx, |this, cx| {
                this.table.update(cx, |table, cx| {
                    table.delegate_mut().apply_persisted_state(&state);
                    table.refresh(cx);
                    cx.notify();
                });
            });
        })
        .detach();
    }

    /// Create an entity bound to a specific dock/workspace.
    pub fn new_entity_in_workspace(
        workspace_id: EntityId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new_in_workspace(workspace_id, window, cx))
    }

    // ── Accessors for the sibling modules ────────────────────

    pub(crate) fn sftp(&self) -> Option<&Arc<dyn SftpBackend>> {
        self.sftp.as_ref()
    }

    pub(crate) fn active_key(&self) -> Option<BackendKey> {
        self.active_key
    }

    pub(crate) fn browser(&self) -> &BrowserView {
        &self.browser
    }

    pub(crate) fn browser_mut(&mut self) -> &mut BrowserView {
        &mut self.browser
    }

    pub(crate) fn transfers(&self) -> &TransferQueueView {
        &self.transfers
    }

    pub(crate) fn transfers_mut(&mut self) -> &mut TransferQueueView {
        &mut self.transfers
    }

    pub(crate) fn follow(&self) -> &FollowCwd {
        &self.follow
    }

    pub(crate) fn follow_mut(&mut self) -> &mut FollowCwd {
        &mut self.follow
    }

    pub(crate) fn table(&self) -> &Entity<TableState<SftpTableDelegate>> {
        &self.table
    }

    pub(crate) fn path_input(&self) -> &Entity<InputState> {
        &self.path_input
    }

    pub(crate) fn panel_focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Attach `backend` as the active session without going through
    /// `AppState` — lets tests script a panel directly.
    #[cfg(test)]
    pub(crate) fn attach_backend_for_test(
        &mut self,
        backend: Arc<dyn SftpBackend>,
        cwd: RemotePath,
        cx: &mut Context<Self>,
    ) {
        let key = SftpBrowserStore::global(cx).track_backend(&backend);
        self.sftp = Some(backend);
        self.active_key = Some(key);
        self.browser.set_cwd(cwd);
        self.ensure_poll_timer(cx);
    }

    // ── Backend switching ────────────────────────────────────

    /// Pull the active SFTP backend + cwd source from this workspace's state.
    ///
    /// Called both from the AppState observer (whenever the active SSH tab changes)
    /// and once at construction. The construction call is what fixes the
    /// "SSH Client → Agent → SSH Client shows NO SFTP connection" bug: a brand-new
    /// `SftpPanel` starts with `sftp: None`, and if the active SSH tab did not
    /// change while it was gone, the observer never fires — so we seed from the
    /// current `AppState` here.
    ///
    /// Per-tab state: when the SFTP backend changes, the panel's current cwd /
    /// entries / sort / selection / transfers / follow-flag / path-input are saved
    /// to the per-backend store under the OLD backend's key, then restored (or
    /// initialized) for the NEW backend. So switching between two SSH tabs keeps
    /// each tab's SFTP browser state (cwd + transfer queue) independent, and a
    /// background transfer keeps running — its progress lives in the store and
    /// reappears when the user switches back to that tab.
    fn sync_from_app_state(
        &mut self,
        new_sftp: Option<Arc<dyn SftpBackend>>,
        new_cwd_source: Option<Arc<dyn CwdSource>>,
        cx: &mut Context<Self>,
    ) {
        // Always track the active terminal's cwd source (may change with the tab
        // even when the SFTP backend does not).
        self.follow.set_source(new_cwd_source);
        self.refresh_terminal_cwd_cache(cx);

        let new_key = super::browser_state::backend_key(&new_sftp);
        if new_key == self.active_key {
            // Same backend (or both None) — nothing to swap.
            return;
        }

        // Preserve live backend state, but never retain a closed session's history.
        if let Some(old_key) = self.active_key {
            if self.sftp.as_ref().is_some_and(|backend| backend.alive()) {
                self.save_state_for_key(old_key, cx);
            } else {
                SftpBrowserStore::global(cx).purge_closed();
            }
        }

        log::info!(
            "SftpPanel: SFTP backend changed — old_key={:?}, new_key={:?}",
            self.active_key,
            new_key
        );

        self.sftp = new_sftp;
        self.active_key = new_key;

        // Restore (or initialize) the NEW backend's view.
        match new_key {
            Some(key) => {
                self.ensure_poll_timer(cx);
                let store = SftpBrowserStore::global(cx);
                if let Some(backend) = self.sftp.as_ref() {
                    store.track_backend(backend);
                }
                let state = store.get_or_default(key);
                let is_fresh = state.browser.cwd().is_empty();
                self.apply_state(state, cx);
                if is_fresh {
                    // First time this backend is shown — load its root dir.
                    log::debug!("SftpPanel: loading initial dir \".\" for new backend");
                    self.load_dir(RemotePath::new("."), cx);
                }
            }
            None => {
                // No SFTP backend (local shell / no SFTP) — show the empty state.
                self.browser = BrowserView::default();
                self.transfers = TransferQueueView::default();
                self.follow.clear();
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().set_entries(Vec::new());
                    t.delegate_mut().loading = false;
                    t.clear_selection(cx);
                    cx.notify();
                });
            }
        }
    }

    /// Snapshot the panel's current view into the store under `key`.
    ///
    /// Reads the delegate's entries + sort (so the file list is preserved exactly),
    fn save_state_for_key(&mut self, key: BackendKey, cx: &mut Context<Self>) {
        let sort = self.table.read(cx).delegate().sort;
        let store = SftpBrowserStore::global(cx);
        let entries = if self.entries_dirty {
            self.table.read(cx).delegate().entries_snapshot()
        } else {
            store.entries(key)
        };
        let (follow_terminal_cwd, last_followed_cwd) = self.follow.snapshot();

        let state = SftpBrowserState {
            browser: self.browser.clone(),
            entries,
            sort,
            transfers: self.transfers.clone(),
            follow_terminal_cwd,
            last_followed_cwd,
        };
        store.save(key, state);
        self.snapshot_gate.clear();
        self.entries_dirty = false;
    }

    /// Mark the active browser projection for one deferred store snapshot.
    pub(crate) fn mark_state_dirty(&mut self) {
        self.snapshot_gate.mark();
    }

    /// Mark the directory entries as changed so the next snapshot refreshes the Arc.
    pub(crate) fn mark_entries_dirty(&mut self) {
        self.entries_dirty = true;
        self.mark_state_dirty();
    }

    /// Drain every pending-change flag; `true` when a snapshot is due.
    fn take_dirty(&mut self) -> bool {
        // Evaluate every flag so none stays raised for the next tick.
        let gate = self.snapshot_gate.take();
        let browser = self.browser.take_dirty();
        let transfers = self.transfers.take_dirty();
        let follow = self.follow.take_dirty();
        gate || browser || transfers || follow
    }

    /// Apply a stored snapshot to the panel + table.
    fn apply_state(&mut self, state: SftpBrowserState, cx: &mut Context<Self>) {
        self.browser = state.browser;
        self.transfers = state.transfers;
        self.follow
            .restore(state.follow_terminal_cwd, state.last_followed_cwd);
        self.take_dirty();
        self.entries_dirty = false;
        self.table.update(cx, |t, cx| {
            t.delegate_mut().sort = state.sort;
            t.delegate_mut().loading = false;
            t.delegate_mut().set_entries(state.entries.to_vec());
            t.clear_selection(cx);
            t.refresh(cx);
            cx.notify();
        });
        // NOTE: the path input value is intentionally not restored here —
        // `set_value` needs a `&mut Window` (not available in this context), and
        // `render` already syncs the path input to `cwd` when the input is not
        // focused. A half-typed path is thus not preserved across a tab switch,
        // which is acceptable.
    }

    // ── Event handlers ───────────────────────────────────────

    fn on_table_event(
        &mut self,
        _: &Entity<TableState<SftpTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TableEvent::SelectRow(idx) => {
                self.browser.select(Some(*idx));
                cx.notify();
            }
            TableEvent::DoubleClickedRow(idx) => {
                log::debug!("SftpPanel: double-click row {idx} → navigate_into");
                self.navigate_into(*idx, cx);
            }
            TableEvent::ClearSelection => {
                self.browser.select(None);
                cx.notify();
            }
            TableEvent::ColumnWidthsChanged(widths) => {
                // Update widths in the delegate + debounce persist.
                let widths: Vec<_> = widths.iter().map(|p| *p).collect();
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().apply_widths(&widths);
                    cx.notify();
                });
                self.mark_state_dirty();
                self.schedule_save_table_state(cx);
            }
            _ => {}
        }
    }

    /// Handler for InputEvent from the path input.
    fn on_path_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => {
                let path = self.path_input.read(cx).value().trim().to_string();
                if path.is_empty() {
                    return;
                }
                self.goto_path(RemotePath::new(path), cx);
            }
            InputEvent::Change => {
                // Reset the error highlight when the user types again.
                if self.browser.path_error() {
                    self.browser.set_path_error(false);
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    /// Goto path — stat it, then `load_dir` when it is a directory; on error, set
    /// `path_error`. The stat result is discarded when a newer directory request
    /// or a backend switch happened while it was in flight.
    pub(crate) fn goto_path(&mut self, path: RemotePath, cx: &mut Context<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => return,
        };
        let generation = self.load_generation;
        let key = self.active_key;
        cx.spawn(async move |this, cx| {
            let result = sftp.stat(path.clone()).await;
            // The panel may be gone before the stat completes; nothing to apply then.
            _ = this.update(cx, |this, cx| {
                if this.load_generation != generation || this.active_key != key {
                    log::debug!("SftpPanel::goto_path: discarding stale result for \"{path}\"");
                    return;
                }
                match result {
                    Ok(stat) if stat.is_dir => {
                        this.browser.set_path_error(false);
                        this.load_dir(path, cx);
                    }
                    Ok(_) => {
                        log::warn!("SftpPanel::goto_path: not a directory: \"{path}\"");
                        this.browser.set_path_error(true);
                        cx.notify();
                    }
                    Err(error) => {
                        log::warn!("SftpPanel::goto_path: invalid path \"{path}\": {error}");
                        this.browser.set_path_error(true);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Bump the listing generation and return the new value (see `load_generation`).
    pub(crate) fn next_load_generation(&mut self) -> u64 {
        self.load_generation = self.load_generation.wrapping_add(1);
        self.load_generation
    }

    /// `true` when a listing started at `generation` for backend `key` is still current.
    pub(crate) fn is_current_load(&self, generation: u64, key: Option<BackendKey>) -> bool {
        self.load_generation == generation && self.active_key == key
    }

    /// Debounce 1s, snapshot the column state (width + visibility) on the UI
    /// thread, then write it to docks.json on the background executor.
    pub(crate) fn schedule_save_table_state(&mut self, cx: &mut Context<Self>) {
        self._save_table_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let Ok(state) = this.update(cx, |this, cx| {
                this.table.read(cx).delegate().to_persisted_state()
            }) else {
                return;
            };
            cx.background_executor()
                .spawn(async move {
                    if let Err(error) = write_sftp_table_state(&state) {
                        log::warn!("SftpPanel: persist table state failed: {error:#}");
                    }
                })
                .await;
        }));
    }
}

// ── Trait impls ──────────────────────────────────────────────

impl EventEmitter<PanelEvent> for SftpPanel {}

impl Focusable for SftpPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for SftpPanel {
    fn panel_name(&self) -> &'static str {
        "sftp"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "SFTP Browser"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        Some(PanelControl::Both)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use gpui::TestAppContext;
    use oneterm_core::RemotePath;

    use super::SftpPanel;
    use crate::test_backend::FakeSftpBackend;

    /// CORR-67: the 500 ms poll timer runs only while a backend is active.
    #[gpui::test]
    fn poll_timer_stops_without_a_backend_and_restarts_with_one(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(oneterm_state::AppState::init);
        cx.update(crate::browser_state::SftpBrowserStore::init);
        let (panel, cx) = cx.add_window_view(|window, cx| SftpPanel::new(window, cx));

        // A fresh panel shows "No SFTP connection" — nothing to poll.
        assert!(!panel.read_with(cx, |panel, _| panel.poll_timer_running()));

        let backend = Arc::new(FakeSftpBackend::new());
        panel.update(cx, |panel, cx| {
            panel.attach_backend_for_test(backend, RemotePath::new("/srv"), cx);
        });
        assert!(panel.read_with(cx, |panel, _| panel.poll_timer_running()));

        // Switching to no backend lets the loop end on its next tick.
        panel.update(cx, |panel, cx| panel.sync_from_app_state(None, None, cx));
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        assert!(!panel.read_with(cx, |panel, _| panel.poll_timer_running()));

        // The next backend restarts it.
        let backend = Arc::new(FakeSftpBackend::new());
        panel.update(cx, |panel, cx| {
            panel.attach_backend_for_test(backend, RemotePath::new("/srv"), cx);
        });
        assert!(panel.read_with(cx, |panel, _| panel.poll_timer_running()));
    }
}
