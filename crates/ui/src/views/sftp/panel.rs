//! [`SftpPanel`] — leaf panel hiển thị SFTP browser.
//!
//! Hiển thị file tree từ remote SFTP server. 1 panel cho toàn app —
//! observe `AppState.active_sftp` để biết SSH tab nào đang active.
//!
//! File list render bằng `gpui_component::table::DataTable`:
//! - Columns resizable, sortable, ẩn/hiện (config).
//! - Name column pinned left + width lớn nhất (ưu tiên độ dài).
//! - Trạng thái cột (width + visibility) persist vào `docks.json`
//!   (field `sftp_table_state`).
//!
//! Tham chiếu `docs/sftp-browser-design.md` §4.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    Subscription, Task, Window,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::table::{TableEvent, TableState};

use myterm2_core::{FileEntry, SftpBackend};

use crate::state::AppState;

use super::table_delegate::SftpTableDelegate;
use super::types::{PendingAction, SortColumn, TransferItem, sftp_changed};

// ── SftpPanel ────────────────────────────────────────────────

/// Panel hiển thị SFTP browser.
///
/// `panel_name = "sftp"`. 1 panel cho toàn app ở right dock.
/// Observe `AppState.active_sftp` — khi SSH tab đổi, swap SFTP backend.
pub struct SftpPanel {
    pub(crate) focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    pub(crate) sftp: Option<Arc<dyn SftpBackend>>,

    // ── File tree state ─────────────────────────────────────
    pub(crate) cwd: PathBuf,
    /// Entries + sort + loading + column config sống trong delegate.
    pub(crate) table: Entity<TableState<SftpTableDelegate>>,
    /// Mirror index dòng đang chọn (sync từ `TableEvent::SelectRow` +
    /// context menu right-click). Dùng cho toolbar actions.
    pub(crate) selected: Option<usize>,
    pub(crate) error: Option<String>,

    // ── Transfer queue state ────────────────────────────────
    pub(crate) transfers: Vec<TransferItem>,
    pub(crate) next_transfer_id: usize,

    // ── Pending action (context menu → render) ──────────────
    pub(crate) pending_action: Option<PendingAction>,

    // ── Debounce save table state ───────────────────────────
    _save_table_task: Option<Task<()>>,
    _table_sub: Subscription,
}

impl SftpPanel {
    /// Tạo panel mới — observe AppState, tạo DataTable state.
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

        Self {
            focus_handle,
            sftp: None,
            cwd: PathBuf::new(),
            table,
            selected: None,
            error: None,
            transfers: Vec::new(),
            next_transfer_id: 0,
            pending_action: None,
            _save_table_task: None,
            _table_sub: table_sub,
        }
    }

    /// Helper tạo `Entity<Self>`.
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
                // Cập nhật width trong delegate + debounce persist.
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

    /// Debounce 1s rồi persist column state (width + visibility) vào docks.json.
    fn schedule_save_table_state(&mut self, cx: &mut Context<Self>) {
        self._save_table_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(1))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.table.read(cx).delegate().persist();
                cx.notify();
            });
        }));
    }

    // ── File operations ──────────────────────────────────────

    /// Đọc thư mục — spawn background task, không block UI.
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

                        // Update cwd với absolute path từ entry đầu tiên.
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

    /// Navigate lên thư mục cha.
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

    /// Refresh thư mục hiện tại.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::refresh: refreshing \"{}\"", self.cwd.display());
        self.load_dir(self.cwd.clone(), cx);
    }

    /// Navigate vào thư mục con (double-click folder).
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

    /// Toggle visibility của 1 cột (từ Columns dropdown). Name không thể ẩn.
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

    /// Get selected entry (if any) — clone để dùng trong dialog.
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