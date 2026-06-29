//! [`SftpPanel`] — leaf panel hiển thị SFTP browser.
//!
//! Hiển thị file tree từ remote SFTP server. 1 panel cho toàn app —
//! observe `AppState.active_sftp` để biết SSH tab nào đang active.
//!
//! Columns: Name, Date Modified, Size, Permissions, Owner, Group.
//! Tất cả sortable. Default: Name asc, folder trước file.
//! Luôn hiện hidden files.
//!
//! Tham chiếu `docs/sftp-browser-design.md` §4.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Window,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};

use myterm2_core::{FileEntry, SftpBackend};

use crate::state::AppState;

use super::types::{PendingAction, SortColumn, SortDir, TransferItem, sftp_changed, sort_entries};

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
    pub(crate) entries: Vec<FileEntry>,
    pub(crate) selected: Option<usize>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,

    // ── Sort state ───────────────────────────────────────────
    pub(crate) sort_col: SortColumn,
    pub(crate) sort_dir: SortDir,

    // ── Transfer queue state ────────────────────────────────
    pub(crate) transfers: Vec<TransferItem>,
    pub(crate) next_transfer_id: usize,

    // ── Pending action (context menu → render) ──────────────
    pub(crate) pending_action: Option<PendingAction>,
}

impl SftpPanel {
    /// Tạo panel mới — observe AppState ngay.
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let app_state = AppState::global(cx);
        log::debug!("SftpPanel::new: observing AppState for active_sftp changes");

        cx.observe(&app_state, |this, state, cx| {
            let new_sftp = state.read(cx).active_sftp.clone();
            if sftp_changed(&this.sftp, &new_sftp) {
                log::info!(
                    "SftpPanel: SFTP backend changed — old={}, new={}",
                    this.sftp.is_some(),
                    new_sftp.is_some()
                );
                this.sftp = new_sftp;
                this.entries.clear();
                this.selected = None;
                this.error = None;
                this.cwd = PathBuf::new();
                this.transfers.clear();
                this.pending_action = None;
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
            entries: Vec::new(),
            selected: None,
            loading: false,
            error: None,
            sort_col: SortColumn::Name,
            sort_dir: SortDir::Asc,
            transfers: Vec::new(),
            next_transfer_id: 0,
            pending_action: None,
        }
    }

    /// Helper tạo `Entity<Self>`.
    pub fn new_entity(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    // ── File operations ──────────────────────────────────────

    /// Đọc thư mục — spawn background task, không block UI.
    pub fn load_dir(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::load_dir: path=\"{}\"", path.display());

        let sftp = match &self.sftp {
            Some(s) => s.clone(),
            None => {
                log::warn!("SftpPanel::load_dir: no SFTP connection — ignoring");
                self.loading = false;
                return;
            }
        };

        self.loading = true;
        self.error = None;
        self.cwd = path.clone();
        self.selected = None;
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
                this.loading = false;
                match result {
                    Ok(mut entries) => {
                        log::info!(
                            "SftpPanel::load_dir: got {} entries for \"{}\"",
                            entries.len(),
                            this.cwd.display()
                        );

                        // Update cwd với absolute path từ entry đầu tiên.
                        if let Some(first) = entries.first() {
                            if let Some(parent) = first.path.parent() {
                                this.cwd = parent.to_path_buf();
                            }
                        }

                        // Sort theo sort state hiện tại.
                        sort_entries(&mut entries, this.sort_col, this.sort_dir);
                        log::debug!(
                            "SftpPanel::load_dir: sorted by {:?} {:?}",
                            this.sort_col,
                            this.sort_dir
                        );

                        this.entries = entries;
                        this.error = None;
                    }
                    Err(e) => {
                        log::error!("SftpPanel::load_dir: read_dir failed: {e}");
                        this.error = Some(e.to_string());
                        this.entries.clear();
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
        match self.entries.get(idx) {
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

    /// Click column header → sort theo cột đó.
    /// Nếu click cùng cột → toggle direction. Nếu click cột khác → sort asc.
    pub(crate) fn sort_by(&mut self, col: SortColumn, cx: &mut Context<Self>) {
        if col == self.sort_col {
            self.sort_dir = self.sort_dir.toggle();
            log::debug!(
                "SftpPanel::sort_by: same col {:?} → dir={:?}",
                col,
                self.sort_dir
            );
        } else {
            self.sort_col = col;
            self.sort_dir = SortDir::Asc;
            log::debug!("SftpPanel::sort_by: new col {:?} → dir=Asc", col);
        }

        // Re-sort entries đã có (không cần reload).
        sort_entries(&mut self.entries, self.sort_col, self.sort_dir);
        self.selected = None;
        cx.notify();
    }

    /// Get selected entry (if any).
    pub(crate) fn selected_entry(&self) -> Option<&FileEntry> {
        self.selected.and_then(|ix| self.entries.get(ix))
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
