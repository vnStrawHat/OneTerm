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

use oneterm_core::SftpBackend;
use oneterm_terminal::CwdSource;

use oneterm_state::AppState;

use super::table_delegate::SftpTableDelegate;
use super::types::{PendingAction, TransferItem, sftp_changed};

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

    // ── Auto-follow terminal cwd ────────────────────────────
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

        // Auto-follow polling timer — checks terminal cwd every 500ms and
        // syncs the SFTP browser if follow is enabled and the cwd changed.
        let _follow_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.maybe_follow_terminal_cwd(cx);
                });
            }
        });

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
            follow_terminal_cwd: false,
            last_followed_cwd: None,
            _follow_task: Some(_follow_task),
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
    pub(crate) fn goto_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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
