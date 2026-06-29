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
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex, input::{Input, InputState}, menu::{ContextMenuExt as _, PopupMenuItem}, progress::Progress, v_flex,
};
use myterm2_core::{FileEntry, FileStat, SftpBackend};

use crate::state::AppState;

// ── Sort state ───────────────────────────────────────────────

/// Cột để sort.
#[derive(Clone, Copy, PartialEq, Debug)]
enum SortColumn {
    Name,
    Modified,
    Size,
    Permissions,
    Owner,
    Group,
}

/// Hướng sort.
#[derive(Clone, Copy, PartialEq, Debug)]
enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    /// Toggle asc ↔ desc.
    fn toggle(self) -> Self {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

// ── Pending action (for context menu → render execution) ─────

/// Action được trigger từ context menu, thực thi trong `render()`.
/// Context menu `on_click` chỉ có `&mut App`, không có `&mut Window`,
/// nên dùng pattern: set flag → render() executes với full `&mut Window` + `&mut Context<Self>`.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PendingAction {
    Open(usize),   // Navigate vào folder
    Download,
    Rename,
    Delete,
    Properties,
    Upload,
    NewFolder,
    Refresh,
}

// ── Helpers: formatting ──────────────────────────────────────

/// Format bytes thành human-readable (B, KB, MB, GB, TB).
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes < 1024 * 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} TB", bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0))
    }
}

/// Format SystemTime thành `YYYY-MM-DD HH:MM` (UTC).
fn format_date(time: Option<SystemTime>) -> String {
    let time = match time {
        Some(t) => t,
        None => return String::new(),
    };
    let dt: DateTime<Utc> = match DateTime::from_timestamp(
        time.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
        0,
    ) {
        Some(dt) => dt,
        None => return String::new(),
    };
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// Format permissions thành `rwxr-xr-x (0775)` — text + octal.
/// Bit layout: owner(rwx) | group(rwx) | other(rwx) | special(sst).
fn format_permissions(perm: u32) -> String {
    let mode = perm & 0o7777; // Chỉ quan tâm 12 bit thấp.

    // Special bits: setuid (4000), setgid (2000), sticky (1000).
    let setuid = mode & 0o4000 != 0;
    let setgid = mode & 0o2000 != 0;
    let sticky = mode & 0o1000 != 0;

    // Owner rwx
    let owner_r = mode & 0o400 != 0;
    let owner_w = mode & 0o200 != 0;
    let owner_x = mode & 0o100 != 0;
    // Group rwx
    let group_r = mode & 0o040 != 0;
    let group_w = mode & 0o020 != 0;
    let group_x = mode & 0o010 != 0;
    // Other rwx
    let other_r = mode & 0o004 != 0;
    let other_w = mode & 0o002 != 0;
    let other_x = mode & 0o001 != 0;

    // Build string: s/r, s/r, t/x for special bits.
    let c = |flag: bool, ch: char| if flag { ch } else { '-' };

    let text = format!(
        "{}{}{}{}{}{}{}{}{}",
        c(owner_r, 'r'),
        c(owner_w, 'w'),
        if owner_x {
            if setuid { 's' } else { 'x' }
        } else if setuid {
            'S'
        } else {
            '-'
        },
        c(group_r, 'r'),
        c(group_w, 'w'),
        if group_x {
            if setgid { 's' } else { 'x' }
        } else if setgid {
            'S'
        } else {
            '-'
        },
        c(other_r, 'r'),
        c(other_w, 'w'),
        if other_x {
            if sticky { 't' } else { 'x' }
        } else if sticky {
            'T'
        } else {
            '-'
        },
    );

    // Octal: 4 digits (mode & 0o7777).
    let octal = format!("{:04o}", mode);
    format!("{text} ({octal})")
}

/// Format owner/group thành `name (id)`. Nếu không có name → chỉ hiển thị `id`.
fn format_owner(name: Option<&str>, id: Option<u32>) -> String {
    match (name, id) {
        (Some(n), Some(id)) => format!("{n} ({id})"),
        (Some(n), None) => n.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => "-".to_string(),
    }
}

/// So sánh 2 `Option<Arc<dyn SftpBackend>>` bằng pointer identity.
fn sftp_changed(
    old: &Option<Arc<dyn SftpBackend>>,
    new: &Option<Arc<dyn SftpBackend>>,
) -> bool {
    match (old, new) {
        (Some(a), Some(b)) => Arc::as_ptr(a) as *const () != Arc::as_ptr(b) as *const (),
        (None, None) => false,
        _ => true,
    }
}

/// Sort entries: folder trước file, trong mỗi nhóm sort theo (col, dir).
fn sort_entries(entries: &mut [FileEntry], col: SortColumn, dir: SortDir) {
    entries.sort_by(|a, b| {
        // Luôn folder trước file.
        let folder_cmp = b.is_dir.cmp(&a.is_dir);
        if folder_cmp != std::cmp::Ordering::Equal {
            return folder_cmp;
        }

        // Cùng loại (cả folder hoặc cả file) → sort theo col.
        let col_cmp = match col {
            SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Permissions => a.permissions.cmp(&b.permissions),
            SortColumn::Owner => a.uid.cmp(&b.uid),
            SortColumn::Group => a.gid.cmp(&b.gid),
        };

        match dir {
            SortDir::Asc => col_cmp,
            SortDir::Desc => col_cmp.reverse(),
        }
    });
}

// ── Column definitions ────────────────────────────────────────

/// Định nghĩa 1 cột trong file list.
struct ColumnDef {
    col: SortColumn,
    label: &'static str,
    /// Chiều rộng cố định (px). None = flex (name).
    width: Option<f32>,
    /// Có right-align text không (size).
    right_align: bool,
}

/// Danh sách cột hiển thị (thứ tự từ trái → phải).
const COLUMNS: &[ColumnDef] = &[
    ColumnDef { col: SortColumn::Name, label: "Name", width: None, right_align: false },
    ColumnDef { col: SortColumn::Modified, label: "Date Modified", width: Some(130.0), right_align: false },
    ColumnDef { col: SortColumn::Size, label: "Size", width: Some(70.0), right_align: true },
    ColumnDef { col: SortColumn::Permissions, label: "Permissions", width: Some(140.0), right_align: false },
    ColumnDef { col: SortColumn::Owner, label: "Owner", width: Some(80.0), right_align: false },
    ColumnDef { col: SortColumn::Group, label: "Group", width: Some(80.0), right_align: false },
];

// ── Transfer queue ──────────────────────────────────────────

/// Hướng transfer.
#[derive(Clone, Copy, PartialEq, Debug)]
enum TransferDirection {
    Upload,
    Download,
}

/// Trạng thái transfer.
#[derive(Clone, Copy, PartialEq, Debug)]
enum TransferStatus {
    InProgress,
    Completed,
    Cancelled,
    Error,
}

/// Một item trong transfer queue.
struct TransferItem {
    id: usize,
    direction: TransferDirection,
    filename: String,
    progress: f64,  // 0.0 – 1.0
    status: TransferStatus,
    error: Option<String>,
}

// ── SftpPanel ────────────────────────────────────────────────

/// Panel hiển thị SFTP browser.
///
/// `panel_name = "sftp"`. 1 panel cho toàn app ở right dock.
/// Observe `AppState.active_sftp` — khi SSH tab đổi, swap SFTP backend.
pub struct SftpPanel {
    focus_handle: FocusHandle,

    // ── SFTP backend state ──────────────────────────────────
    sftp: Option<Arc<dyn SftpBackend>>,

    // ── File tree state ─────────────────────────────────────
    cwd: PathBuf,
    entries: Vec<FileEntry>,
    selected: Option<usize>,
    loading: bool,
    error: Option<String>,

    // ── Sort state ───────────────────────────────────────────
    sort_col: SortColumn,
    sort_dir: SortDir,

    // ── Transfer queue state ────────────────────────────────
    transfers: Vec<TransferItem>,
    next_transfer_id: usize,

    // ── Pending action (context menu → render) ──────────────
    pending_action: Option<PendingAction>,
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
    fn navigate_parent(&mut self, cx: &mut Context<Self>) {
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
    fn refresh(&mut self, cx: &mut Context<Self>) {
        log::debug!("SftpPanel::refresh: refreshing \"{}\"", self.cwd.display());
        self.load_dir(self.cwd.clone(), cx);
    }

    /// Navigate vào thư mục con (double-click folder).
    fn navigate_into(&mut self, idx: usize, cx: &mut Context<Self>) {
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
    fn sort_by(&mut self, col: SortColumn, cx: &mut Context<Self>) {
        if col == self.sort_col {
            self.sort_dir = self.sort_dir.toggle();
            log::debug!("SftpPanel::sort_by: same col {:?} → dir={:?}", col, self.sort_dir);
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

    // ── File operations (toolbar) ───────────────────────────

    /// Get selected entry (if any).
    fn selected_entry(&self) -> Option<&FileEntry> {
        self.selected.and_then(|ix| self.entries.get(ix))
    }

    /// Rename selected entry.
    /// Mở dialog với InputState pre-fill tên hiện tại → sftp.rename().
    fn do_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry() {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_rename: no selection");
                window.push_notification("Select a file or folder to rename.", cx);
                return;
            }
        };

        log::info!("SftpPanel::do_rename: \"{}\"", entry.name);

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let from_path = entry.path.clone();

        let name_state = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("New name");
            st.set_value(&entry.name, window, cx);
            st
        });

        let name_ok = name_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let name_ok = name_ok.clone();
            let sftp = sftp.clone();
            let from_path = from_path.clone();
            let panel = panel.clone();
            move |_, window, cx| {
                let new_name = name_ok.read(cx).value().trim().to_string();
                if new_name.is_empty() {
                    window.push_notification("Name cannot be empty.", cx);
                    return false;
                }

                // Build new path: parent + new_name
                let parent = from_path.parent().unwrap_or_else(|| std::path::Path::new("/"));
                let to_path = parent.join(&new_name);

                log::info!("SftpPanel: rename \"{}\" → \"{}\"", from_path.display(), to_path.display());

                match sftp.rename(from_path.clone(), to_path) {
                    Ok(()) => {
                        log::info!("SftpPanel: rename OK");
                        window.push_notification(format!("Renamed to \"{new_name}\"."), cx);
                        panel.update(cx, |this, cx| this.refresh(cx));
                        true
                    }
                    Err(e) => {
                        log::error!("SftpPanel: rename failed: {e}");
                        window.push_notification(format!("Rename failed: {e}"), cx);
                        false
                    }
                }
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("Rename")
                .w(px(440.))
                .content({
                    let name_state = name_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex().gap_1().w_full()
                                .child(div().text_sm().text_color(cx.theme().foreground).child("New name"))
                                .child(Input::new(&name_state))
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(
                            Button::new("cancel").label("Cancel").outline()
                                .on_click(|_, window, cx| { window.close_dialog(cx); })
                        )
                        .child(
                            Button::new("save").label("Rename").primary()
                                .on_click(move |_, window, cx| {
                                    if save_for_click(&ClickEvent::default(), window, cx) {
                                        window.close_dialog(cx);
                                    }
                                })
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| { save_for_kb(&ClickEvent::default(), window, cx) }),
                )
        });
    }

    /// Delete selected entry (file or folder).
    /// Mở alert dialog confirm → sftp.remove() hoặc sftp.rmdir().
    fn do_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry() {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_delete: no selection");
                window.push_notification("Select a file or folder to delete.", cx);
                return;
            }
        };

        log::info!("SftpPanel::do_delete: \"{}\" (is_dir={})", entry.name, entry.is_dir);

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let path = entry.path.clone();
        let is_dir = entry.is_dir;
        let entry_name = entry.name.clone();
        let kind_str = if is_dir { "folder" } else { "file" };
        let desc = format!("Are you sure you want to delete {kind_str} \"{entry_name}\"?");

        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            alert
                .confirm()
                .title("Confirm Delete")
                .description(desc.clone())
                .footer({
                    let sftp = sftp.clone();
                    let path = path.clone();
                    let panel = panel.clone();
                    DialogFooter::new()
                        .child(
                            Button::new("cancel").label("Cancel").outline()
                                .on_click(|_, window, cx| { window.close_dialog(cx); })
                        )
                        .child(
                            Button::new("delete").label("Delete").danger()
                                .on_click(move |_, window, cx| {
                                    log::info!("SftpPanel: deleting \"{}\"", path.display());
                                    let result = if is_dir {
                                        sftp.rmdir(path.clone())
                                    } else {
                                        sftp.remove(path.clone())
                                    };
                                    match result {
                                        Ok(()) => {
                                            log::info!("SftpPanel: delete OK");
                                            window.push_notification("Deleted successfully.", cx);
                                            panel.update(cx, |this, cx| this.refresh(cx));
                                            window.close_dialog(cx);
                                        }
                                        Err(e) => {
                                            log::error!("SftpPanel: delete failed: {e}");
                                            window.push_notification(format!("Delete failed: {e}"), cx);
                                        }
                                    }
                                })
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(|_, _, _| false),
                )
        });
    }

    /// Tạo thư mục mới trong cwd.
    /// Mở dialog với InputState → sftp.mkdir().
    fn do_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("SftpPanel::do_new_folder: cwd=\"{}\"", self.cwd.display());

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let cwd = self.cwd.clone();

        let name_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Folder name")
        });

        let name_ok = name_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let name_ok = name_ok.clone();
            let sftp = sftp.clone();
            let cwd = cwd.clone();
            let panel = panel.clone();
            move |_, window, cx| {
                let name = name_ok.read(cx).value().trim().to_string();
                if name.is_empty() {
                    window.push_notification("Folder name cannot be empty.", cx);
                    return false;
                }
                let path = cwd.join(&name);
                log::info!("SftpPanel: mkdir \"{}\"", path.display());
                match sftp.mkdir(path) {
                    Ok(()) => {
                        log::info!("SftpPanel: mkdir OK");
                        window.push_notification(format!("Folder \"{name}\" created."), cx);
                        panel.update(cx, |this, cx| this.refresh(cx));
                        true
                    }
                    Err(e) => {
                        log::error!("SftpPanel: mkdir failed: {e}");
                        window.push_notification(format!("Create folder failed: {e}"), cx);
                        false
                    }
                }
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("New Folder")
                .w(px(440.))
                .content({
                    let name_state = name_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex().gap_1().w_full()
                                .child(div().text_sm().text_color(cx.theme().foreground).child("Folder name"))
                                .child(Input::new(&name_state))
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(
                            Button::new("cancel").label("Cancel").outline()
                                .on_click(|_, window, cx| { window.close_dialog(cx); })
                        )
                        .child(
                            Button::new("create").label("Create").primary()
                                .on_click(move |_, window, cx| {
                                    if save_for_click(&ClickEvent::default(), window, cx) {
                                        window.close_dialog(cx);
                                    }
                                })
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| { save_for_kb(&ClickEvent::default(), window, cx) }),
                )
        });
    }

    /// Upload file local → remote.
    /// Mở dialog nhập local path → sftp.upload() → poll progress trong background.
    fn do_upload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("SftpPanel::do_upload: cwd=\"{}\"", self.cwd.display());

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let cwd = self.cwd.clone();

        let path_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("C:\\path\\to\\local\\file.txt")
        });

        let path_ok = path_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let path_ok = path_ok.clone();
            let sftp = sftp.clone();
            let cwd = cwd.clone();
            let panel = panel.clone();
            move |_, window, cx| {
                let local = path_ok.read(cx).value().trim().to_string();
                if local.is_empty() {
                    window.push_notification("Local file path cannot be empty.", cx);
                    return false;
                }
                let local_path = PathBuf::from(&local);

                // Remote path: cwd / filename
                let filename = local_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "uploaded".to_string());
                let remote_path = cwd.join(&filename);

                log::info!("SftpPanel: upload \"{}\" → \"{}\"", local_path.display(), remote_path.display());

                // Add TransferItem to panel — get transfer_id trước khi gọi upload.
                let panel_clone = panel.clone();
                let transfer_id = panel.update(cx, |this, cx| {
                    let id = this.next_transfer_id;
                    this.next_transfer_id += 1;
                    this.transfers.push(TransferItem {
                        id,
                        direction: TransferDirection::Upload,
                        filename: filename.clone(),
                        progress: 0.0,
                        status: TransferStatus::InProgress,
                        error: None,
                    });
                    log::debug!("SftpPanel: added transfer #{id} upload \"{filename}\"");
                    cx.notify();
                    id
                });

                // Gọi upload với transfer_id (để có thể cancel).
                let (progress_rx, result_rx) = sftp.upload(transfer_id as u64, local_path, remote_path);

                window.push_notification(format!("Uploading \"{filename}\"..."), cx);

                // Clone panel for spawn — save_logic is Fn, can be called multiple times.
                let panel = panel_clone.clone();
                // Poll progress trong background → update TransferItem.
                cx.spawn(async move |cx| {
                    while let Ok(progress) = progress_rx.recv().await {
                        // progress = -1.0 → cancelled signal.
                        if progress < 0.0 {
                            log::info!("SftpPanel: upload #{transfer_id} cancelled");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Cancelled;
                                    }
                                    cx.notify();
                                });
                            });
                            return; // ← exit spawn task, không đợi result.
                        }
                        log::debug!("SftpPanel: upload #{transfer_id} progress {:.0}%", progress * 100.0);
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                    item.progress = progress;
                                    cx.notify();
                                }
                            });
                        });
                    }
                    match result_rx.recv().await {
                        Ok(Ok(())) => {
                            log::info!("SftpPanel: upload #{transfer_id} OK");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Completed;
                                        item.progress = 1.0;
                                    }
                                    this.refresh(cx);
                                    cx.notify();
                                });
                            });
                        }
                        Ok(Err(e)) => {
                            // Check nếu error là "cancelled" → đã handle ở trên, skip.
                            if e.to_string() == "cancelled" {
                                return;
                            }
                            log::error!("SftpPanel: upload #{transfer_id} failed: {e}");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Error;
                                        item.error = Some(e.to_string());
                                    }
                                    cx.notify();
                                });
                            });
                        }
                        Err(_) => {
                            log::error!("SftpPanel: upload #{transfer_id} result channel closed");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Error;
                                        item.error = Some("channel closed".to_string());
                                    }
                                    cx.notify();
                                });
                            });
                        }
                    }
                }).detach();

                true
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("Upload File")
                .w(px(440.))
                .content({
                    let path_state = path_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex().gap_1().w_full()
                                .child(div().text_sm().text_color(cx.theme().foreground).child("Local file path"))
                                .child(Input::new(&path_state))
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(
                            Button::new("cancel").label("Cancel").outline()
                                .on_click(|_, window, cx| { window.close_dialog(cx); })
                        )
                        .child(
                            Button::new("upload").label("Upload").primary()
                                .on_click(move |_, window, cx| {
                                    if save_for_click(&ClickEvent::default(), window, cx) {
                                        window.close_dialog(cx);
                                    }
                                })
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| { save_for_kb(&ClickEvent::default(), window, cx) }),
                )
        });
    }

    /// Download file remote → local.
    /// Mở dialog nhập local save path → sftp.download() → poll progress.
    fn do_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry() {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_download: no selection");
                window.push_notification("Select a file to download.", cx);
                return;
            }
        };

        if entry.is_dir {
            log::warn!("SftpPanel::do_download: cannot download directory");
            window.push_notification("Cannot download a folder. Select a file.", cx);
            return;
        }

        log::info!("SftpPanel::do_download: \"{}\"", entry.name);

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let remote_path = entry.path.clone();
        let entry_name = entry.name.clone();

        let path_state = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("C:\\path\\to\\save\\here");
            st.set_value(&entry_name, window, cx);
            st
        });

        let path_ok = path_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let path_ok = path_ok.clone();
            let sftp = sftp.clone();
            let panel = panel.clone();
            let remote_path = remote_path.clone();
            let entry_name = entry_name.clone();
            move |_, window, cx| {
                let local = path_ok.read(cx).value().trim().to_string();
                if local.is_empty() {
                    window.push_notification("Local save path cannot be empty.", cx);
                    return false;
                }
                let local_path = PathBuf::from(&local);

                log::info!("SftpPanel: download \"{}\" → \"{}\"", remote_path.display(), local_path.display());

                // Add TransferItem to panel — get transfer_id trước khi gọi download.
                let transfer_id = panel.update(cx, |this, cx| {
                    let id = this.next_transfer_id;
                    this.next_transfer_id += 1;
                    this.transfers.push(TransferItem {
                        id,
                        direction: TransferDirection::Download,
                        filename: entry_name.clone(),
                        progress: 0.0,
                        status: TransferStatus::InProgress,
                        error: None,
                    });
                    log::debug!("SftpPanel: added transfer #{id} download \"{entry_name}\"");
                    cx.notify();
                    id
                });

                // Gọi download với transfer_id (để có thể cancel).
                let (progress_rx, result_rx) = sftp.download(transfer_id as u64, remote_path.clone(), local_path);

                window.push_notification(format!("Downloading \"{entry_name}\"..."), cx);

                // Clone panel for spawn — save_logic is Fn, can be called multiple times.
                let panel = panel.clone();
                cx.spawn(async move |cx| {
                    while let Ok(progress) = progress_rx.recv().await {
                        // progress = -1.0 → cancelled signal.
                        if progress < 0.0 {
                            log::info!("SftpPanel: download #{transfer_id} cancelled");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Cancelled;
                                    }
                                    cx.notify();
                                });
                            });
                            return; // ← exit spawn task.
                        }
                        log::debug!("SftpPanel: download #{transfer_id} progress {:.0}%", progress * 100.0);
                        cx.update(|cx| {
                            panel.update(cx, |this, cx| {
                                if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                    item.progress = progress;
                                    cx.notify();
                                }
                            });
                        });
                    }
                    match result_rx.recv().await {
                        Ok(Ok(())) => {
                            log::info!("SftpPanel: download #{transfer_id} OK");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Completed;
                                        item.progress = 1.0;
                                    }
                                    cx.notify();
                                });
                            });
                        }
                        Ok(Err(e)) => {
                            if e.to_string() == "cancelled" {
                                return;
                            }
                            log::error!("SftpPanel: download #{transfer_id} failed: {e}");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Error;
                                        item.error = Some(e.to_string());
                                    }
                                    cx.notify();
                                });
                            });
                        }
                        Err(_) => {
                            log::error!("SftpPanel: download #{transfer_id} result channel closed");
                            cx.update(|cx| {
                                panel.update(cx, |this, cx| {
                                    if let Some(item) = this.transfers.iter_mut().find(|t| t.id == transfer_id) {
                                        item.status = TransferStatus::Error;
                                        item.error = Some("channel closed".to_string());
                                    }
                                    cx.notify();
                                });
                            });
                        }
                    }
                }).detach();

                true
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("Download File")
                .w(px(440.))
                .content({
                    let path_state = path_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex().gap_1().w_full()
                                .child(div().text_sm().text_color(cx.theme().foreground).child("Local save path"))
                                .child(Input::new(&path_state))
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(
                            Button::new("cancel").label("Cancel").outline()
                                .on_click(|_, window, cx| { window.close_dialog(cx); })
                        )
                        .child(
                            Button::new("download").label("Download").primary()
                                .on_click(move |_, window, cx| {
                                    if save_for_click(&ClickEvent::default(), window, cx) {
                                        window.close_dialog(cx);
                                    }
                                })
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| { save_for_kb(&ClickEvent::default(), window, cx) }),
                )
        });
    }


    /// Show properties dialog — sftp.stat() → hiển thị metadata chi tiết.
    fn do_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry() {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_properties: no selection");
                window.push_notification("Select a file or folder to view properties.", cx);
                return;
            }
        };

        log::info!("SftpPanel::do_properties: \"{}\"", entry.name);

        let sftp = self.sftp.clone().unwrap();
        let stat: FileStat = match sftp.stat(entry.path.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("SftpPanel: stat failed: {e}");
                window.push_notification(format!("Failed to get properties: {e}"), cx);
                return;
            }
        };

        log::debug!("SftpPanel: stat OK — size={}, perm={:#o}, uid={:?}, gid={:?}",
            stat.size, stat.permissions, stat.uid, stat.gid);

        // Build detail rows — wrap in Rc for sharing across Fn closures.
        let kind_str = if stat.is_dir { "Folder" } else { "File" };
        let size_text = Rc::new(if stat.is_dir {
            "-".to_string()
        } else {
            format!("{} ({} bytes)", format_size(stat.size), stat.size)
        });
        let modified_text = Rc::new(format_date(stat.modified));
        let accessed_text = Rc::new(format_date(stat.accessed));
        let perm_text = Rc::new(format_permissions(stat.permissions));
        let owner_text = Rc::new(format_owner(stat.owner.as_deref(), stat.uid));
        let group_text = Rc::new(format_owner(stat.group.as_deref(), stat.gid));
        let path_text = Rc::new(stat.path.display().to_string());
        let name_text = Rc::new(stat.name.clone());
        let is_symlink = stat.is_symlink;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            // Clone Rc values here so content closure can capture them by move.
            let name_text = name_text.clone();
            let size_text = size_text.clone();
            let modified_text = modified_text.clone();
            let accessed_text = accessed_text.clone();
            let perm_text = perm_text.clone();
            let owner_text = owner_text.clone();
            let group_text = group_text.clone();
            let path_text = path_text.clone();

            dialog
                .title("Properties")
                .w(px(480.))
                .content(move |content, _window, cx| {
                    let theme = cx.theme();
                    let label_w = px(100.0);
                    let muted = theme.muted_foreground;

                    // Helper: label + value row.
                    let row = |label: &str, value: String| {
                        h_flex()
                            .w_full()
                            .gap_2()
                            .py_1()
                            .child(
                                div()
                                    .w(label_w)
                                    .flex_shrink_0()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(label.to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(theme.foreground)
                                    .truncate()
                                    .child(value),
                            )
                    };

                    content.child(
                        v_flex()
                            .gap_0()
                            .w_full()
                            .child(row("Name:", (*name_text).clone()))
                            .child(row("Type:", format!("{kind_str}{}", if is_symlink { " (symlink)" } else { "" })))
                            .child(row("Size:", (*size_text).clone()))
                            .child(row("Modified:", (*modified_text).clone()))
                            .child(row("Accessed:", (*accessed_text).clone()))
                            .child(row("Permissions:", (*perm_text).clone()))
                            .child(row("Owner:", (*owner_text).clone()))
                            .child(row("Group:", (*group_text).clone()))
                            .child(row("Path:", (*path_text).clone())),
                    )
                })
                .footer({
                    DialogFooter::new().child(
                        Button::new("close")
                            .label("Close")
                            .primary()
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(|_, window, cx| {
                            window.close_dialog(cx);
                            true
                        }),
                )
        });
    }



    /// Render khi không có SFTP connection.
    fn render_no_connection(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No SFTP connection.")
    }

    /// Render breadcrumb: path + ↑ parent + ⟳ refresh.
    fn render_breadcrumb(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hover_bg = muted.opacity(0.1);

        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("sftp-parent-btn")
                    .cursor_pointer()
                    .px_1()
                    .py_0()
                    .rounded(px(3.))
                    .hover(move |t| t.bg(hover_bg))
                    .child(Icon::new(IconName::ArrowUp).xsmall().text_color(muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigate_parent(cx);
                    })),
            )
            .child(
                div()
                    .id("sftp-refresh-btn")
                    .cursor_pointer()
                    .px_1()
                    .py_0()
                    .rounded(px(3.))
                    .hover(move |t| t.bg(hover_bg))
                    .child(Icon::new(IconName::Redo).xsmall().text_color(muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh(cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(muted)
                    .child(self.cwd.display().to_string()),
            )
    }

    /// Render toolbar: Upload, Download, Rename, Delete, New Folder buttons.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger_fg = theme.danger_foreground;

        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            // New Folder
            .child(
                Button::new("sftp-new-folder")
                    .label("New Folder")
                    .icon(Icon::new(IconName::Plus).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_new_folder(window, cx);
                    })),
            )
            // Upload
            .child(
                Button::new("sftp-upload")
                    .label("Upload")
                    .icon(Icon::new(IconName::ArrowUp).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_upload(window, cx);
                    })),
            )
            // Download
            .child(
                Button::new("sftp-download")
                    .label("Download")
                    .icon(Icon::new(IconName::ArrowDown).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_download(window, cx);
                    })),
            )
            // Rename
            .child(
                Button::new("sftp-rename")
                    .label("Rename")
                    .icon(Icon::new(IconName::Replace).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_rename(window, cx);
                    })),
            )
            // Delete (danger)
            .child(
                Button::new("sftp-delete")
                    .label("Delete")
                    .icon(Icon::new(IconName::Delete).xsmall())
                    .small()
                    .ghost()
                    .text_color(danger_fg)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_delete(window, cx);
                    })),
            )
            // Properties
            .child(
                Button::new("sftp-properties")
                    .label("Properties")
                    .icon(Icon::new(IconName::Info).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_properties(window, cx);
                    })),
            )
            // Spacer
            .child(div().flex_1())
            // Selection info
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        self.selected
                            .map(|ix| {
                                self.entries.get(ix)
                                    .map(|e| format!("{} selected", e.name))
                                    .unwrap_or_else(|| "? selected".to_string())
                            })
                            .unwrap_or_else(|| "No selection".to_string()),
                    ),
            )
    }


    /// Clear completed and errored transfers.
    fn clear_completed_transfers(&mut self, cx: &mut Context<Self>) {
        let before = self.transfers.len();
        self.transfers.retain(|t| t.status == TransferStatus::InProgress);
        let removed = before - self.transfers.len();
        if removed > 0 {
            log::debug!("SftpPanel: cleared {removed} completed/errored transfers");
        }
        cx.notify();
    }

    /// Render transfer queue — hiển thị progress cho ongoing transfers.
    /// Chỉ render khi self.transfers không rỗng.
    fn render_transfer_queue(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.transfers.is_empty() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let accent = theme.accent;
        let danger = theme.danger;

        // Count active vs completed
        let active_count = self.transfers.iter().filter(|t| t.status == TransferStatus::InProgress).count();
        let completed_count = self.transfers.len() - active_count;

        let mut queue = v_flex()
            .w_full()
            .flex_shrink_0()
            .max_h(px(200.0))
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.15));

        // Header: "Transfers" + count + Clear button
        queue = queue.child(
            h_flex()
                .w_full()
                .h_7()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .px_2()
                .child(
                    div().text_xs().text_color(muted)
                        .child("Transfers"),
                )
                .child(
                    div().text_xs().text_color(muted)
                        .child(format!("{active_count} active, {completed_count} done")),
                )
                .child(div().flex_1())
                .child(
                    Button::new("sftp-clear-transfers")
                        .label("Clear")
                        .xsmall()
                        .ghost()
                        .disabled(completed_count == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_completed_transfers(cx);
                        })),
                )
        );

        // Transfer items
        let mut list = v_flex().id("sftp-transfer-list").w_full().overflow_y_scroll();

        for item in &self.transfers {
            // Direction icon
            let dir_icon = match item.direction {
                TransferDirection::Upload => Icon::new(IconName::ArrowUp).xsmall().text_color(accent),
                TransferDirection::Download => Icon::new(IconName::ArrowDown).xsmall().text_color(accent),
            };

            // Status indicator color
            let progress_color = match item.status {
                TransferStatus::InProgress => accent,
                TransferStatus::Completed => theme.success,
                TransferStatus::Cancelled => muted,
                TransferStatus::Error => danger,
            };

            list = list.child(
                h_flex()
                    .w_full()
                    .h_6()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .px_2()
                    // Direction icon
                    .child(div().w_4().flex_shrink_0().child(dir_icon))
                    // Filename
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(item.filename.clone()),
                    )
                    // Progress bar
                    .child(
                        div()
                            .w(px(80.0))
                            .flex_shrink_0()
                            .child(
                                Progress::new(gpui::ElementId::NamedInteger("sftp-transfer".into(), item.id as u64))
                                    .xsmall()
                                    .color(progress_color)
                                    .value((item.progress * 100.0) as f32)
                            ),
                    )
                    // Percentage + status
                    .child(
                        div()
                            .w(px(50.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(match item.status {
                                TransferStatus::InProgress => format!("{:.0}%", item.progress * 100.0),
                                TransferStatus::Completed => "Done".to_string(),
                                TransferStatus::Cancelled => "Cancelled".to_string(),
                                TransferStatus::Error => "Error".to_string(),
                            }),
                    )
                    // Error message (if any)
                    .when(item.error.is_some(), |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(danger)
                                .truncate()
                                .child(item.error.clone().unwrap_or_default()),
                        )
                    })
                    // Cancel button — chỉ hiển thị khi InProgress.
                    .when(item.status == TransferStatus::InProgress, |this| {
                        let cancel_id = item.id;
                        this.child(
                            div()
                                .flex_shrink_0()
                                .child(
                                    Button::new(gpui::ElementId::NamedInteger("sftp-cancel-transfer".into(), item.id as u64))
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .tooltip("Cancel transfer")
                                        .on_click(cx.listener(move |this_ref, _, _, cx| {
                                            log::info!("SftpPanel: cancel transfer #{cancel_id}");
                                            if let Some(ref sftp) = this_ref.sftp.clone() {
                                                sftp.cancel_transfer(cancel_id as u64);
                                            }
                                            if let Some(t) = this_ref.transfers.iter_mut().find(|t| t.id == cancel_id) {
                                                t.status = TransferStatus::Cancelled;
                                                cx.notify();
                                            }
                                        })),
                                ),
                        )
                    }),
            );
        }

        queue = queue.child(list);
        queue.into_any_element()
    }
    /// Render column headers — clickable để sort, có sort indicator.
    fn render_column_headers(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let accent = theme.accent;
        let header_bg = theme.muted.opacity(0.3);
        let hover_bg = theme.muted.opacity(0.5);

        let mut header = h_flex()
            .w_full()
            .h_6()
            .flex_shrink_0()
            .items_center()
            .px_2()
            .gap_2()
            .bg(header_bg)
            .border_b_1()
            .border_color(theme.border);

        for (i, col_def) in COLUMNS.iter().enumerate() {
            let is_active_sort = self.sort_col == col_def.col;
            let sort_dir = self.sort_dir;

            // Sort indicator icon.
            let sort_icon = if is_active_sort {
                match sort_dir {
                    SortDir::Asc => Some(Icon::new(IconName::ChevronUp).xsmall().text_color(accent)),
                    SortDir::Desc => Some(Icon::new(IconName::ChevronDown).xsmall().text_color(accent)),
                }
            } else {
                None
            };

            // Text color: accent if active sort, muted otherwise.
            let text_color = if is_active_sort { accent } else { muted };

            // Build cell.
            let label = col_def.label.to_string();
            let col = col_def.col;
            let right_align = col_def.right_align;

            let mut cell = div()
                .id(gpui::ElementId::NamedInteger("sftp-col".into(), i as u64))
                .h_full()
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .rounded(px(2.))
                .hover(move |t| t.bg(hover_bg))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sort_by(col, cx);
                }));

            if right_align {
                cell = cell.justify_end();
            }

            // Fixed width or flex.
            if let Some(w) = col_def.width {
                cell = cell.w(px(w)).flex_shrink_0();
            } else {
                cell = cell.flex_1().min_w_0();
            }

            // Label text.
            cell = cell.child(
                div()
                    .text_xs()
                    .text_color(text_color)
                    .child(label),
            );

            // Sort indicator.
            if let Some(icon) = sort_icon {
                cell = cell.child(icon);
            }

            header = header.child(cell);
        }

        header.into_any_element()
    }

    /// Render 1 row trong file list.
    fn render_entry_row(
        &self,
        idx: usize,
        entry: &FileEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_selected = self.selected == Some(idx);

        let icon = if entry.is_dir {
            Icon::new(IconName::Folder)
                .xsmall()
                .text_color(theme.foreground)
        } else {
            Icon::new(IconName::File)
                .xsmall()
                .text_color(theme.muted_foreground)
        };

        let selected_bg = theme.accent.opacity(0.1);
        let row_bg = if is_selected {
            selected_bg
        } else {
            gpui::hsla(0.0, 0.0, 0.0, 0.0)
        };

        let name_color = if entry.is_dir {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let name_text = entry.name.clone();
        let name_for_log = name_text.clone();

        // Format cell values.
        let date_text = format_date(entry.modified);
        let size_text = if entry.is_dir {
            String::new()
        } else {
            format_size(entry.size)
        };
        let perm_text = format_permissions(entry.permissions);
        let owner_text = format_owner(entry.owner.as_deref(), entry.uid);
        let group_text = format_owner(entry.group.as_deref(), entry.gid);

        // Cell text color: muted for all columns except name.
        let muted = theme.muted_foreground;

        v_flex()
            .id(gpui::ElementId::NamedInteger(
                "sftp-entry".into(),
                idx as u64,
            ))
            .w_full()
            .h_7()
            .flex_shrink_0()
            .cursor_pointer()
            .bg(row_bg)
            .hover(move |t| t.bg(theme.accent.opacity(0.05)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                let click_count = event.click_count();
                log::debug!(
                    "SftpPanel: click entry {idx} (count={click_count}) — \"{name}\"",
                    name = name_for_log
                );
                this.selected = Some(idx);
                if click_count >= 2 {
                    log::debug!("SftpPanel: double-click → navigate_into({idx})");
                    this.navigate_into(idx, cx);
                }
                cx.notify();
            }))
            // Context menu — right-click trên entry row.
            .context_menu({
                let panel = cx.entity();
                let is_dir = entry.is_dir;
                move |menu, _window: &mut Window, cx| {
                    // Select entry on right-click.
                    panel.update(cx, |this, cx| {
                        this.selected = Some(idx);
                        cx.notify();
                    });

                    log::debug!("SftpPanel: context menu for entry {idx} (is_dir={is_dir})");

                    // Build menu — first item depends on type.
                    let menu = if is_dir {
                        menu.item(PopupMenuItem::new("Open").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Open(idx));
                                    cx.notify();
                                });
                            }
                        }))
                    } else {
                        menu.item(PopupMenuItem::new("Download").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Download);
                                    cx.notify();
                                });
                            }
                        }))
                    };

                    menu
                        .separator()
                        .item(PopupMenuItem::new("Rename").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Rename);
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Delete").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Delete);
                                    cx.notify();
                                });
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Properties").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Properties);
                                    cx.notify();
                                });
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Upload").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Upload);
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("New Folder").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::NewFolder);
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Refresh").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Refresh);
                                    cx.notify();
                                });
                            }
                        }))
                }
            })
            // Row content: h_flex with all columns matching header widths.
            .child(
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .gap_2()
                    .px_2()
                    // ── Name column (flex, truncated) ──
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_1()
                            .child(div().w_4().flex_shrink_0().child(icon))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(name_color)
                                    .truncate() // overflow_hidden + nowrap + ellipsis
                                    .child(name_text.clone()),
                            ),
                    )
                    // ── Date Modified column (130px) ──
                    .child(
                        div()
                            .w(px(130.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(date_text),
                    )
                    // ── Size column (70px, right-aligned) ──
                    .child(
                        div()
                            .w(px(70.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .text_align(gpui::TextAlign::Right)
                            .child(size_text),
                    )
                    // ── Permissions column (140px) ──
                    .child(
                        div()
                            .w(px(140.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(perm_text),
                    )
                    // ── Owner column (80px) ──
                    .child(
                        div()
                            .w(px(80.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(owner_text),
                    )
                    // ── Group column (80px) ──
                    .child(
                        div()
                            .w(px(80.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(group_text),
                    ),
            )
    }

    /// Render file list — column headers + scrollable rows.
    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        if self.loading {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Loading...")
                .into_any_element();
        }

        if let Some(err) = &self.error {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .text_color(theme.danger_foreground)
                .child(format!("Error: {err}"))
                .into_any_element();
        }

        if self.entries.is_empty() {
            let panel = cx.entity();
            return div()
                .id("sftp-empty-area")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Empty directory.")
                .context_menu({
                    let panel = panel.clone();
                    move |menu, _window: &mut Window, _cx| {
                        log::debug!("SftpPanel: context menu for empty area");
                        menu
                            .item(PopupMenuItem::new("Upload").on_click({
                                let panel = panel.clone();
                                move |_, _window, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Upload);
                                        cx.notify();
                                    });
                                }
                            }))
                            .item(PopupMenuItem::new("New Folder").on_click({
                                let panel = panel.clone();
                                move |_, _window, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::NewFolder);
                                        cx.notify();
                                    });
                                }
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Refresh").on_click({
                                let panel = panel.clone();
                                move |_, _window, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Refresh);
                                        cx.notify();
                                    });
                                }
                            }))
                    }
                })
                .into_any_element();
        }

        // Column headers (fixed, not scrollable).
        let headers = self.render_column_headers(cx);

        // Scrollable file list.
        let mut list = v_flex()
            .id("sftp-file-list")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();

        for (idx, entry) in self.entries.iter().enumerate() {
            list = list.child(self.render_entry_row(idx, entry, cx));
        }

        v_flex()
            .flex_1()
            .min_h_0()
            .child(headers)
            .child(list)
            .into_any_element()
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

impl Render for SftpPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Execute pending action from context menu.
        if let Some(action) = self.pending_action.take() {
            log::debug!("SftpPanel: executing pending action: {action:?}");
            match action {
                PendingAction::Open(idx) => self.navigate_into(idx, cx),
                PendingAction::Download => self.do_download(_window, cx),
                PendingAction::Rename => self.do_rename(_window, cx),
                PendingAction::Delete => self.do_delete(_window, cx),
                PendingAction::Properties => self.do_properties(_window, cx),
                PendingAction::Upload => self.do_upload(_window, cx),
                PendingAction::NewFolder => self.do_new_folder(_window, cx),
                PendingAction::Refresh => self.refresh(cx),
            }
        }

        if self.sftp.is_none() {
            return self.render_no_connection(cx).into_any_element();
        }

        let theme = cx.theme();

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .child(self.render_breadcrumb(cx))
            .child(self.render_toolbar(cx))
            .child(self.render_transfer_queue(cx))
            .child(self.render_file_list(cx))
            .into_any_element()
    }
}