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
//! See `docs/sftp-browser-design.md` §4.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    Subscription, Task, Window,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::table::{TableEvent, TableState};
use oneterm_ui::dock::{Panel, PanelControl, PanelEvent};

use oneterm_core::SftpBackend;
use oneterm_terminal::CwdSource;

use oneterm_state::AppState;

use super::table_delegate::SftpTableDelegate;
use super::types::{PendingAction, TransferItem};

// ── SftpPanel ────────────────────────────────────────────────

/// Panel displaying the SFTP browser.
///
/// `panel_name = "sftp"`. One panel for the whole app in the right dock.
/// Observes `AppState.active_sftp` — when the SSH tab changes, swap the SFTP backend.
pub struct SftpPanel {
    pub(crate) focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,
    /// Stable store key for the active backend. `None` = no SFTP backend
    /// (local shell). Tracked so the panel knows which store entry
    /// owns the currently-displayed cwd/entries/transfers.
    pub(crate) active_key: Option<super::browser_state::BackendKey>,

    /// Live cwd source of the active terminal tab (OSC 7). Read on demand by the
    /// "sync to terminal cwd" toolbar button. `None` = no cwd available.
    pub(crate) cwd_source: Option<Arc<dyn CwdSource>>,

    // ── File tree state (active view; mirrored from the store on tab switch) ─
    pub(crate) cwd: PathBuf,
    /// Entries + sort + loading + column config live in the delegate.
    pub(crate) table: Entity<TableState<SftpTableDelegate>>,
    /// Mirror of the selected row index (synced from `TableEvent::SelectRow` +
    /// context-menu right-click). Used by toolbar actions.
    pub(crate) selected: Option<usize>,
    pub(crate) error: Option<String>,

    // ── Transfer queue (active view; mirrored from the store on tab switch).
    /// The source of truth is the per-backend store (`SftpBrowserStore`); this
    /// vec mirrors the active backend's queue so render can read it. Background
    /// transfer tasks update the store by `(backend_key, transfer_id)` and, when
    /// that key is active, also update this vec so the UI re-renders live.
    pub(crate) transfers: Vec<TransferItem>,
    pub(crate) next_transfer_id: usize,

    // ── Pending action (context menu → render) ──────────────
    pub(crate) pending_action: Option<PendingAction>,

    // ── Path input (toolbar) ────────────────────────────────
    pub(crate) path_input: Entity<InputState>,
    pub(crate) path_error: bool,
    _path_sub: Subscription,

    // ── Auto-follow terminal cwd (active view; mirrored from the store) ────
    /// When enabled, the SFTP browser automatically navigates to the terminal's
    /// cwd whenever it changes (OSC 7). Toggled via the "..." menu checkbox.
    pub(crate) follow_terminal_cwd: bool,
    /// The last terminal cwd we followed to — used by the polling timer to
    /// detect changes (avoids redundant `read_dir` when the cwd hasn't moved).
    pub(crate) last_followed_cwd: Option<PathBuf>,
    /// Handle for the auto-follow polling task so we can detach it.
    _follow_task: Option<Task<()>>,

    // ── Debounce save table state ───────────────────────────
    _save_table_task: Option<Task<()>>,
    _table_sub: Subscription,
}

impl SftpPanel {
    /// Create a new panel — observe AppState, create DataTable state.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let app_state = AppState::global(cx);
        log::debug!("SftpPanel::new: observing AppState for active_sftp changes");

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

        cx.observe(&app_state, |this, state, cx| {
            let (sftp, cwd_source) = {
                let s = state.read(cx);
                (s.active_sftp.clone(), s.active_cwd_source.clone())
            };
            this.sync_from_app_state(sftp, cwd_source, cx);
            cx.notify();
        })
        .detach();

        // Auto-follow polling timer — checks terminal cwd every 500ms and
        // syncs the SFTP browser if follow is enabled and the cwd changed.
        let _follow_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.maybe_follow_terminal_cwd(cx);
                    // Persist the active backend's view to the store so the latest
                    // cwd/entries/sort/selection/follow-flag survive even if the
                    // SftpPanel is destroyed without a tab switch (e.g. the right
                    // dock is swapped via the mode toggle: SSH Client → Agent drops
                    // the panel — the store keeps each backend's last state).
                    if let Some(key) = this.active_key {
                        this.save_state_for_key(key, cx);
                    }
                });
            }
        });

        // Path input — display cwd, Enter → goto path.
        let path_input = cx.new(|cx| InputState::new(window, cx).placeholder("Path"));
        let _path_sub = cx.subscribe_in(&path_input, window, Self::on_path_input_event);

        let mut me = Self {
            focus_handle,
            sftp: None,
            active_key: None,
            cwd_source: None,
            cwd: PathBuf::new(),
            table,
            selected: None,
            error: None,
            transfers: Vec::new(),
            next_transfer_id: 0,
            pending_action: None,
            path_input,
            path_error: false,
            _path_sub,
            follow_terminal_cwd: false,
            last_followed_cwd: None,
            _follow_task: Some(_follow_task),
            _save_table_task: None,
            _table_sub: table_sub,
        };

        // Seed from the current AppState so a freshly-created panel (e.g. after
        // switching SSH Client → Agent → SSH Client) shows the active SSH tab's
        // SFTP connection immediately. The observer above only fires on *change*,
        // so without this seed the panel stays empty when the active tab is the
        // same one that was active before the swap.
        let (sftp, cwd_source) = {
            let s = app_state.read(cx);
            (s.active_sftp.clone(), s.active_cwd_source.clone())
        };
        me.sync_from_app_state(sftp, cwd_source, cx);

        me
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    // ── Table event handler ──────────────────────────────────

    /// Pull the active SFTP backend + cwd source from `AppState`.
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
        self.cwd_source = new_cwd_source;

        let new_key = super::browser_state::backend_key(&new_sftp);
        if new_key == self.active_key {
            // Same backend (or both None) — nothing to swap.
            return;
        }

        // Save the current view into the OLD backend's store entry.
        if let Some(old_key) = self.active_key {
            self.save_state_for_key(old_key, cx);
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
                let store = super::browser_state::SftpBrowserStore::global(cx);
                let state = store.get_or_default(key);
                let is_fresh = state.cwd.as_os_str().is_empty();
                self.apply_state(state, cx);
                if is_fresh {
                    // First time this backend is shown — load its root dir.
                    log::debug!("SftpPanel: loading initial dir \".\" for new backend");
                    self.load_dir(PathBuf::from("."), cx);
                }
            }
            None => {
                // No SFTP backend (local shell / no SFTP) — show the empty state.
                self.selected = None;
                self.error = None;
                self.cwd = PathBuf::new();
                self.transfers.clear();
                self.next_transfer_id = 0;
                self.pending_action = None;
                self.follow_terminal_cwd = false;
                self.last_followed_cwd = None;
                self.path_error = false;
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().entries.clear();
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
    fn save_state_for_key(&self, key: super::browser_state::BackendKey, cx: &mut Context<Self>) {
        let (entries, sort) = {
            let d = self.table.read(cx).delegate();
            (d.entries.clone(), d.sort)
        };

        let state = super::browser_state::SftpBrowserState {
            cwd: self.cwd.clone(),
            entries,
            sort,
            selected: self.selected,
            error: self.error.clone(),
            transfers: self.transfers.clone(),
            next_transfer_id: self.next_transfer_id,
            pending_action: self.pending_action,
            follow_terminal_cwd: self.follow_terminal_cwd,
            last_followed_cwd: self.last_followed_cwd.clone(),
            path_error: self.path_error,
        };
        super::browser_state::SftpBrowserStore::global(cx).save(key, state);
    }

    /// Apply a stored snapshot to the panel + table.
    fn apply_state(
        &mut self,
        state: super::browser_state::SftpBrowserState,
        cx: &mut Context<Self>,
    ) {
        self.cwd = state.cwd;
        self.selected = state.selected;
        self.error = state.error;
        self.transfers = state.transfers;
        self.next_transfer_id = state.next_transfer_id;
        self.pending_action = state.pending_action;
        self.follow_terminal_cwd = state.follow_terminal_cwd;
        self.last_followed_cwd = state.last_followed_cwd;
        self.path_error = state.path_error;
        self.table.update(cx, |t, cx| {
            t.delegate_mut().sort = state.sort;
            t.delegate_mut().loading = false;
            t.delegate_mut().set_entries(state.entries);
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

    // ── Transfer queue helpers (used by transfer.rs tasks) ──────────

    /// Allocate a new transfer id for the ACTIVE backend and push the item both
    /// into the panel's active view AND the store (so it survives tab switches
    /// while the task runs). Returns the allocated id, or `None` if no SFTP
    /// backend is active.
    pub(crate) fn push_transfer(
        &mut self,
        item: TransferItem,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let key = self.active_key?;
        let id = item.id;
        let store = super::browser_state::SftpBrowserStore::global(cx);
        // Push into the store (source of truth).
        store.with_mut(key, |st| st.transfers.push(item.clone()));
        // Mirror into the active view.
        self.transfers.push(item);
        self.next_transfer_id = self.next_transfer_id.saturating_add(1);
        cx.notify();
        Some(id)
    }

    /// Update a transfer (by id) for the backend identified by `key` in the
    /// store, and mirror the updated item into the active view when `key` is the
    /// active backend. The closure `f` runs once against the store entry; the
    /// active view's matching item is then overwritten with the store's updated
    /// copy (so the UI re-renders without calling `f` twice).
    pub(crate) fn update_transfer_for(
        &mut self,
        key: super::browser_state::BackendKey,
        transfer_id: usize,
        f: impl FnOnce(&mut TransferItem),
        cx: &mut Context<Self>,
    ) -> bool {
        let store = super::browser_state::SftpBrowserStore::global(cx);
        let mut updated: Option<TransferItem> = None;
        let found = store.with_mut(key, |st| {
            if let Some(item) = st.transfers.iter_mut().find(|t| t.id == transfer_id) {
                f(item);
                updated = Some(item.clone());
                return true;
            }
            false
        });
        if let Some(item) = updated {
            if self.active_key == Some(key) {
                if let Some(view_item) = self.transfers.iter_mut().find(|t| t.id == transfer_id) {
                    *view_item = item;
                }
                cx.notify();
            }
        }
        found
    }

    /// Allocate+return the next transfer id for the active backend (counter is
    /// per-backend in the store; this updates the active view's mirror too).
    pub(crate) fn alloc_transfer_id(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        let key = self.active_key?;
        let id = {
            let store = super::browser_state::SftpBrowserStore::global(cx);
            store.with_mut(key, |st| {
                let i = st.next_transfer_id;
                st.next_transfer_id = i.saturating_add(1);
                i
            })
        };
        self.next_transfer_id = id.saturating_add(1);
        Some(id)
    }

    fn on_table_event(
        &mut self,
        _: &Entity<TableState<SftpTableDelegate>>,
        event: &TableEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TableEvent::SelectRow(idx) => {
                self.selected = Some(*idx);
                cx.notify();
            }
            TableEvent::DoubleClickedRow(idx) => {
                log::debug!("SftpPanel: double-click row {idx} → navigate_into");
                self.navigate_into(*idx, cx);
            }
            TableEvent::ClearSelection => {
                self.selected = None;
                cx.notify();
            }
            TableEvent::ColumnWidthsChanged(widths) => {
                // Update widths in the delegate + debounce persist.
                let widths: Vec<_> = widths.iter().map(|p| *p).collect();
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().apply_widths(&widths);
                    cx.notify();
                });
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
                self.goto_path(PathBuf::from(path), cx);
            }
            InputEvent::Change => {
                // Reset the error highlight when the user types again.
                if self.path_error {
                    self.path_error = false;
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    /// Goto path — try read_dir; on error, set path_error.
    pub(crate) fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => return,
        };
        let _ = cx.spawn(async move |this, cx| {
            let result = sftp.stat(path.clone()).await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(stat) if stat.is_dir => {
                    this.path_error = false;
                    this.load_dir(path, cx);
                }
                Ok(_) => {
                    log::warn!(
                        "SftpPanel::goto_path: not a directory: \"{}\"",
                        path.display()
                    );
                    this.path_error = true;
                    cx.notify();
                }
                Err(error) => {
                    log::warn!(
                        "SftpPanel::goto_path: invalid path \"{}\": {error}",
                        path.display()
                    );
                    this.path_error = true;
                    cx.notify();
                }
            });
        });
    }

    /// Debounce 1s then persist column state (width + visibility) to docks.json.
    pub(crate) fn schedule_save_table_state(&mut self, cx: &mut Context<Self>) {
        self._save_table_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, cx| {
                this.table.read(cx).delegate().persist();
                cx.notify();
            });
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
