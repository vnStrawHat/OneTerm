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
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use gpui::{
    App, AppContext, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable,
    dock::{Panel, PanelControl, PanelEvent},
    h_flex, v_flex,
};
use myterm2_core::{FileEntry, SftpBackend};

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

    // ── Render helpers ────────────────────────────────────────

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
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Empty directory.")
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
            .child(self.render_file_list(cx))
            .into_any_element()
    }
}