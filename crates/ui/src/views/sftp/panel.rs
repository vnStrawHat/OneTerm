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
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::table::{TableEvent, TableState};

use oneterm_core::{CwdSource, FileEntry, SftpBackend};

use crate::state::AppState;

use super::table_delegate::SftpTableDelegate;
use super::types::{PendingAction, SortColumn, TransferItem, sftp_changed};

// ── SftpPanel ────────────────────────────────────────────────

/// Panel displaying the SFTP browser.
///
/// `panel_name = "sftp"`. One panel for the whole app in the right dock.
/// Observes `AppState.active_sftp` — when the SSH tab changes, swap the SFTP backend.
pub struct SftpPanel {
    pub(crate) focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,

    /// Live cwd source of the active terminal tab (OSC 7). Read on demand by the
    /// "sync to terminal cwd" toolbar button. `None` = no cwd available.
    pub(crate) cwd_source: Option<Arc<dyn CwdSource>>,

    // ── File tree state ─────────────────────────────────────
    pub(crate) cwd: PathBuf,
    /// Entries + sort + loading + column config live in the delegate.
    pub(crate) table: Entity<TableState<SftpTableDelegate>>,
    /// Mirror of the selected row index (synced from `TableEvent::SelectRow` +
    /// context-menu right-click). Used by toolbar actions.
    pub(crate) selected: Option<usize>,
    pub(crate) error: Option<String>,

    // ── Transfer queue state ────────────────────────────────
    pub(crate) transfers: Vec<TransferItem>,
    pub(crate) next_transfer_id: usize,

    // ── Pending action (context menu → render) ──────────────
    pub(crate) pending_action: Option<PendingAction>,

    // ── Path input (toolbar) ────────────────────────────────
    pub(crate) path_input: Entity<InputState>,
    pub(crate) path_error: bool,
    _path_sub: Subscription,

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
            let new_sftp = state.read(cx).active_sftp.clone();
            // Always track the active terminal's cwd source (may change with the tab
            // even when the SFTP backend does not).
            this.cwd_source = state.read(cx).active_cwd_source.clone();
            if sftp_changed(&this.sftp, &new_sftp) {
                log::info!(
                    "SftpPanel: SFTP backend changed — old={}, new={}",
                    this.sftp.is_some(),
                    new_sftp.is_some()
                );
                this.sftp = new_sftp;
                this.selected = None;
                this.error = None;
                this.cwd = PathBuf::new();
                this.transfers.clear();
                this.pending_action = None;
                this.table.update(cx, |t, cx| {
                    t.delegate_mut().entries.clear();
                    t.delegate_mut().loading = false;
                    t.clear_selection(cx);
                    cx.notify();
                });
                if this.sftp.is_some() {
                    log::debug!("SftpPanel: loading initial dir \".\"");
                    this.load_dir(PathBuf::from("."), cx);
                }
            }
            cx.notify();
        })
        .detach();

        // Path input — display cwd, Enter → goto path.
        let path_input = cx.new(|cx| InputState::new(window, cx).placeholder("Path"));
        let _path_sub = cx.subscribe_in(&path_input, window, Self::on_path_input_event);

        Self {
            focus_handle,
            sftp: None,
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
            _save_table_task: None,
            _table_sub: table_sub,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    // ── Table event handler ──────────────────────────────────

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
    fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => return,
        };
        // Check path exists via stat — if fails or not a dir, highlight error.
        match sftp.stat(path.clone()) {
            Ok(stat) if stat.is_dir => {
                self.path_error = false;
                self.load_dir(path, cx);
            }
            Ok(_) => {
                log::warn!(
                    "SftpPanel::goto_path: not a directory: \"{}\"",
                    path.display()
                );
                self.path_error = true;
                cx.notify();
            }
            Err(e) => {
                log::warn!(
                    "SftpPanel::goto_path: invalid path \"{}\": {}",
                    path.display(),
                    e
                );
                self.path_error = true;
                cx.notify();
            }
        }
    }

    /// Debounce 1s then persist column state (width + visibility) to docks.json.
    fn schedule_save_table_state(&mut self, cx: &mut Context<Self>) {
        self._save_table_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, cx| {
                this.table.read(cx).delegate().persist();
                cx.notify();
            });
        }));
    }

    // ── File operations ──────────────────────────────────────

    /// Read a directory — spawn a background task, does not block the UI.
    pub fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::load_dir: path=\"{}\"", path.display());

        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                log::warn!("SftpPanel::load_dir: no SFTP connection — ignoring");
                self.table.update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    cx.notify();
                });
                return;
            }
        };

        self.table.update(cx, |t, cx| {
            t.delegate_mut().loading = true;
            cx.notify();
        });
        self.error = None;
        self.cwd = path.clone();
        self.selected = None;
        self.table.update(cx, |t, cx| t.clear_selection(cx));
        cx.notify();

        cx.spawn(async move |this, cx| {
            log::debug!(
                "SftpPanel::load_dir: spawning background read_dir for \"{}\"",
                path.display()
            );

            let result = cx
                .background_executor()
                .spawn(async move { sftp.read_dir(path) })
                .await;

            this.update(cx, |this, cx| {
                this.table.update(cx, |t, cx| {
                    t.delegate_mut().loading = false;
                    cx.notify();
                });
                match result {
                    Ok(entries) => {
                        log::info!(
                            "SftpPanel::load_dir: got {} entries for \"{}\"",
                            entries.len(),
                            this.cwd.display()
                        );

                        // Update cwd with the absolute path from the first entry.
                        let mut cwd = this.cwd.clone();
                        if let Some(first) = entries.first() {
                            if let Some(parent) = first.path.parent() {
                                cwd = parent.to_path_buf();
                            }
                        }
                        this.cwd = cwd;

                        this.table.update(cx, |t, cx| {
                            t.delegate_mut().set_entries(entries);
                            t.refresh(cx);
                        });
                        this.error = None;
                    }
                    Err(e) => {
                        log::error!("SftpPanel::load_dir: read_dir failed: {e}");
                        this.error = Some(e.to_string());
                        this.table.update(cx, |t, cx| {
                            t.delegate_mut().entries.clear();
                            t.refresh(cx);
                        });
                    }
                }
                cx.notify();
            })
        })
        .detach();
    }

    /// Navigate up to the parent directory.
    pub(crate) fn navigate_parent(&mut self, cx: &mut Context<Self>) {
        let parent = match self.cwd.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => {
                log::debug!("SftpPanel::navigate_parent: already at root");
                return;
            }
        };
        log::debug!(
            "SftpPanel::navigate_parent: \"{}\" → \"{}\"",
            self.cwd.display(),
            parent.display()
        );
        self.load_dir(parent, cx);
    }

    /// Refresh the current directory.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::refresh: refreshing \"{}\"", self.cwd.display());
        self.load_dir(self.cwd.clone(), cx);
    }

    /// The current working directory of the active terminal (OSC 7), read live.
    /// Used to compute the "sync" button's enabled state + tooltip.
    pub(crate) fn terminal_cwd(&self) -> Option<PathBuf> {
        self.cwd_source.as_ref().and_then(|s| s.cwd())
    }

    /// Navigate the SFTP browser to the active terminal's current directory.
    /// No-op if there is no SFTP connection or the terminal has not reported a cwd.
    pub(crate) fn sync_to_terminal_cwd(&mut self, cx: &mut Context<Self>) {
        if self.sftp.is_none() {
            return;
        }
        let cwd = match self.terminal_cwd() {
            Some(p) => p,
            None => {
                log::debug!("SftpPanel::sync_to_terminal_cwd: terminal cwd unavailable");
                return;
            }
        };
        log::info!(
            "SftpPanel::sync_to_terminal_cwd: \"{}\" → \"{}\"",
            self.cwd.display(),
            cwd.display()
        );
        // `goto_path` stats the path (dir check) + handles errors + load_dir.
        self.goto_path(cwd, cx);
    }

    /// Navigate into a subdirectory (double-click a folder).
    pub(crate) fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) {
        let entry = self.table.read(cx).delegate().entries.get(idx).cloned();
        match entry {
            Some(entry) if entry.is_dir => {
                log::debug!(
                    "SftpPanel::navigate_into: \"{}\" → \"{}\"",
                    self.cwd.display(),
                    entry.path.display()
                );
                self.load_dir(entry.path.clone(), cx);
            }
            Some(_) => {
                log::debug!("SftpPanel::navigate_into: entry {idx} is not a directory");
            }
            None => {
                log::warn!("SftpPanel::navigate_into: index {idx} out of range");
            }
        }
    }

    /// Toggle the visibility of a column (from the Columns dropdown). Name cannot be hidden.
    pub(crate) fn toggle_column(&mut self, col: SortColumn, cx: &mut Context<Self>) {
        let changed = self.table.update(cx, |t, cx| {
            let changed = t.delegate_mut().toggle_visibility(col);
            if changed {
                t.refresh(cx);
            }
            changed
        });
        if changed {
            self.schedule_save_table_state(cx);
            cx.notify();
        }
    }

    /// Get selected entry (if any) — cloned for use in a dialog.
    pub(crate) fn selected_entry(&self, cx: &App) -> Option<FileEntry> {
        self.selected
            .and_then(|ix| self.table.read(cx).delegate().entries.get(ix).cloned())
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
