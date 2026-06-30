//! Types + helpers cho SFTP browser — sort state, transfer queue,
//! column definitions, formatting.
//!
//! Tách từ `file_browser.rs` để giảm độ dài file.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use myterm2_core::{FileEntry, SftpBackend};

// ── Sort state ───────────────────────────────────────────────

/// Cột để sort.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SortColumn {
    Name,
    Modified,
    Size,
    Permissions,
    Owner,
    Group,
}

/// Hướng sort.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    /// Toggle asc ↔ desc.
    #[allow(dead_code)]
    pub(crate) fn toggle(self) -> Self {
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
pub(crate) enum PendingAction {
    Open(usize), // Navigate vào folder
    Download,
    Rename,
    Delete,
    Properties,
    UploadFiles,
    UploadFolder,
    NewFolder,
    Refresh,
}

// ── Helpers: formatting ──────────────────────────────────────

/// Format bytes thành human-readable (B, KB, MB, GB, TB).
pub(crate) fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes < 1024 * 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!(
            "{:.1} TB",
            bytes as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0)
        )
    }
}

/// Format SystemTime thành `YYYY-MM-DD HH:MM` (UTC).
pub(crate) fn format_date(time: Option<SystemTime>) -> String {
    let time = match time {
        Some(t) => t,
        None => return String::new(),
    };
    let dt: DateTime<Utc> = match DateTime::from_timestamp(
        time.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        0,
    ) {
        Some(dt) => dt,
        None => return String::new(),
    };
    dt.format("%Y-%m-%d %H:%M").to_string()
}

/// Format permissions thành `drwxr-xr-x (0775)` — type + text + octal.
/// Bit layout: file type (high bits) | owner(rwx) | group(rwx) | other(rwx) | special(sst).
pub(crate) fn format_permissions(perm: u32) -> String {
    let mode = perm & 0o7777; // Chỉ quan tâm 12 bit thấp.

    // File type prefix từ high bits (S_IFMT).
    let type_char = match perm & 0o170000 {
        0o040000 => 'd',  // S_IFDIR  — directory
        0o120000 => 'l',  // S_IFLNK  — symlink
        0o020000 => 'c',  // S_IFCHR  — char device
        0o060000 => 'b',  // S_IFBLK  — block device
        0o010000 => 'p',  // S_IFIFO  — pipe/FIFO
        0o140000 => 's',  // S_IFSOCK — socket
        _ => '-',         // S_IFREG hoặc không xác định
    };

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
        "{}{}{}{}{}{}{}{}{}{}",
        type_char,
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
pub(crate) fn format_owner(name: Option<&str>, id: Option<u32>) -> String {
    match (name, id) {
        (Some(n), Some(id)) => format!("{n} ({id})"),
        (Some(n), None) => n.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => "-".to_string(),
    }
}

/// So sánh 2 `Option<Arc<dyn SftpBackend>>` bằng pointer identity.
pub(crate) fn sftp_changed(
    old: &Option<Arc<dyn SftpBackend>>,
    new: &Option<Arc<dyn SftpBackend>>,
) -> bool {
    match (old, new) {
        (Some(a), Some(b)) => Arc::as_ptr(a) as *const () != Arc::as_ptr(b) as *const (),
        (None, None) => false,
        _ => true,
    }
}

/// Sort entries: folder trước file, trong mỗi nhóm sort theo `sort` state.
///
/// `sort = None` → default: sort theo Name asc (folder-first). `Some((col, dir))`
/// → sort theo cột đó. Folder luôn đứng trước file bất kể sort state.
pub(crate) fn sort_entries(entries: &mut [FileEntry], sort: Option<(SortColumn, SortDir)>) {
    let (col, dir) = sort.unwrap_or((SortColumn::Name, SortDir::Asc));
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

impl SortColumn {
    /// Stable string key — dùng cho persistence (docks.json) và Column key.
    pub(crate) fn key(self) -> &'static str {
        match self {
            SortColumn::Name => "name",
            SortColumn::Modified => "modified",
            SortColumn::Size => "size",
            SortColumn::Permissions => "permissions",
            SortColumn::Owner => "owner",
            SortColumn::Group => "group",
        }
    }

    /// Parse key ngược lại `SortColumn`. `None` nếu key không hợp lệ.
    #[allow(dead_code)]
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "name" => SortColumn::Name,
            "modified" => SortColumn::Modified,
            "size" => SortColumn::Size,
            "permissions" => SortColumn::Permissions,
            "owner" => SortColumn::Owner,
            "group" => SortColumn::Group,
            _ => return None,
        })
    }
}

/// Định nghĩa 1 cột trong file list — bao gồm config hiển thị + trạng thái
/// resize/visibility (được persist vào `docks.json`).
#[derive(Clone, Debug)]
pub(crate) struct SftpColumnConfig {
    pub col: SortColumn,
    /// Sortable key string — trùng `SortColumn::key`.
    pub key: &'static str,
    /// Nhãn header.
    pub label: &'static str,
    /// Chiều rộng mặc định (px) — dùng để reset.
    #[allow(dead_code)]
    pub default_width: f32,
    /// Chiều rộng tối thiểu (px) — giới hạn resize.
    pub min_width: f32,
    /// Chiều rộng tối đa (px) — giới hạn resize.
    pub max_width: f32,
    /// Right-align text (Size).
    pub right_align: bool,
    /// Đang hiển thị hay không (config ẩn hiện cột).
    pub visible: bool,
    /// Chiều rộng hiện tại (px) — có thể thay đổi khi resize.
    pub width: f32,
}

impl SftpColumnConfig {
    fn new(
        col: SortColumn,
        label: &'static str,
        default_width: f32,
        right_align: bool,
    ) -> Self {
        Self {
            key: col.key(),
            col,
            label,
            default_width,
            min_width: 40.0,
            max_width: 800.0,
            right_align,
            visible: true,
            width: default_width,
        }
    }
}

/// Danh sách cột canonical (thứ tự từ trái → phải). Name luôn visible.
///
/// Name được ưu tiên độ dài lớn nhất —DataTable dùng fixed-width columns,
/// nên gán width lớn cho name để chiếm nhiều không gian nhất (resizable).
pub(crate) fn default_column_configs() -> Vec<SftpColumnConfig> {
    vec![
        SftpColumnConfig::new(SortColumn::Name, "Name", 320.0, false),
        SftpColumnConfig::new(SortColumn::Modified, "Date Modified", 140.0, false),
        SftpColumnConfig::new(SortColumn::Permissions, "Permissions", 150.0, false),
        SftpColumnConfig::new(SortColumn::Size, "Size", 80.0, true),
        SftpColumnConfig::new(SortColumn::Owner, "Owner", 90.0, false),
        SftpColumnConfig::new(SortColumn::Group, "Group", 90.0, false),
    ]
}

/// Map `SortDir` sang `ColumnSort` của DataTable.
pub(crate) fn sort_dir_to_column_sort(dir: SortDir) -> gpui_component::table::ColumnSort {
    match dir {
        SortDir::Asc => gpui_component::table::ColumnSort::Ascending,
        SortDir::Desc => gpui_component::table::ColumnSort::Descending,
    }
}

// ── Persistence (docks.json field `sftp_table_state`) ─────────

/// Trạng thái bảng SFTP được persist vào `docks.json`.
/// - `column_widths`: key = `SortColumn::key()`, value = px.
/// - `column_visibility`: key = `SortColumn::key()`, value = visible?.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SftpTableStateJson {
    #[serde(default)]
    pub column_widths: HashMap<String, f32>,
    #[serde(default)]
    pub column_visibility: HashMap<String, bool>,
}

// ── Transfer queue ──────────────────────────────────────────

/// Hướng transfer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum TransferDirection {
    Upload,
    Download,
}

/// Trạng thái transfer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum TransferStatus {
    InProgress,
    Completed,
    Cancelled,
    Error,
}

/// Một item trong transfer queue.
pub(crate) struct TransferItem {
    pub id: usize,
    pub direction: TransferDirection,
    pub filename: String,
    pub progress: f64, // 0.0 – 1.0
    pub status: TransferStatus,
    pub error: Option<String>,
}
