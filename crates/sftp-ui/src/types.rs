//! Types + helpers for the SFTP browser — sort state, transfer queue,
//! column definitions, formatting.
//!
//! Split out from `file_browser.rs` to keep the file shorter.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, Utc};

use oneterm_core::FileEntry;

// ── Sort state ───────────────────────────────────────────────

/// Column to sort by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SortColumn {
    Name,
    Modified,
    Size,
    Permissions,
    Owner,
    Group,
}

/// Sort direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SortDir {
    Asc,
    Desc,
}

// ── Helpers: formatting ──────────────────────────────────────

/// Format bytes into human-readable form (B, KB, MB, GB, TB).
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

/// Format SystemTime into `YYYY-MM-DD HH:MM` (local time).
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
    let local = dt.with_timezone(&Local);
    local.format("%Y-%m-%d %H:%M").to_string()
}

/// Format permissions into `drwxr-xr-x (0775)` — type + text + octal.
/// Bit layout: file type (high bits) | owner(rwx) | group(rwx) | other(rwx) | special(sst).
pub(crate) fn format_permissions(perm: u32) -> String {
    let mode = perm & 0o7777; // Only the low 12 bits matter.

    // File type prefix from the high bits (S_IFMT).
    let type_char = match perm & 0o170000 {
        0o040000 => 'd', // S_IFDIR  — directory
        0o120000 => 'l', // S_IFLNK  — symlink
        0o020000 => 'c', // S_IFCHR  — char device
        0o060000 => 'b', // S_IFBLK  — block device
        0o010000 => 'p', // S_IFIFO  — pipe/FIFO
        0o140000 => 's', // S_IFSOCK — socket
        _ => '-',        // S_IFREG or unknown
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

/// Format owner/group into `name (id)`. If there is no name → display only the `id`.
pub(crate) fn format_owner(name: Option<&str>, id: Option<u32>) -> String {
    match (name, id) {
        (Some(n), Some(id)) => format!("{n} ({id})"),
        (Some(n), None) => n.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => "-".to_string(),
    }
}

/// Sort entries: folders before files; within each group sort by the `sort` state.
///
/// `sort = None` → default: sort by Name asc (folder-first). `Some((col, dir))`
/// → sort by that column. Folders always come before files regardless of sort state.
///
/// Name sorting is case-insensitive; the lowercase key is computed once per
/// entry (`sort_by_cached_key`) rather than twice per comparison.
pub(crate) fn sort_entries(entries: &mut [FileEntry], sort: Option<(SortColumn, SortDir)>) {
    let (col, dir) = sort.unwrap_or((SortColumn::Name, SortDir::Asc));
    // Folders first: `!is_dir` sorts `true` (files) after `false` (folders),
    // independent of the direction applied to the column key.
    match col {
        SortColumn::Name => {
            entries.sort_by_cached_key(|e| (!e.is_dir, Directed(e.name.to_lowercase(), dir)))
        }
        SortColumn::Modified => entries.sort_by_key(|e| (!e.is_dir, Directed(e.modified, dir))),
        SortColumn::Size => entries.sort_by_key(|e| (!e.is_dir, Directed(e.size, dir))),
        SortColumn::Permissions => {
            entries.sort_by_key(|e| (!e.is_dir, Directed(e.permissions, dir)))
        }
        SortColumn::Owner => entries.sort_by_key(|e| (!e.is_dir, Directed(e.uid, dir))),
        SortColumn::Group => entries.sort_by_key(|e| (!e.is_dir, Directed(e.gid, dir))),
    }
}

/// A sort key that orders ascending or descending according to its direction.
#[derive(PartialEq, Eq)]
struct Directed<K: Ord>(K, SortDir);

impl<K: Ord> PartialOrd for Directed<K> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<K: Ord> Ord for Directed<K> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.1 {
            SortDir::Asc => self.0.cmp(&other.0),
            SortDir::Desc => other.0.cmp(&self.0),
        }
    }
}

// ── Column definitions ────────────────────────────────────────

impl SortColumn {
    /// Stable string key — used for persistence (docks.json) and the Column key.
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
}

/// Definition of a column in the file list — display config + resize/visibility
/// state (persisted to `docks.json`).
#[derive(Clone, Debug)]
pub(crate) struct SftpColumnConfig {
    pub col: SortColumn,
    /// Sortable key string — matches `SortColumn::key`.
    pub key: &'static str,
    /// Header label.
    pub label: &'static str,
    /// Minimum width (px) — resize limit.
    pub min_width: f32,
    /// Maximum width (px) — resize limit.
    pub max_width: f32,
    /// Right-align text (Size).
    pub right_align: bool,
    /// Whether the column is currently shown (show/hide config).
    pub visible: bool,
    /// Current width (px) — may change on resize.
    pub width: f32,
}

impl SftpColumnConfig {
    fn new(col: SortColumn, label: &'static str, default_width: f32, right_align: bool) -> Self {
        Self {
            key: col.key(),
            col,
            label,
            min_width: 40.0,
            max_width: 800.0,
            right_align,
            visible: true,
            width: default_width,
        }
    }
}

/// Canonical column list (order left → right). Name is always visible.
///
/// Name gets the largest width priority — DataTable uses fixed-width columns,
/// so assign Name a large width to take up the most space (resizable).
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

/// Map `SortDir` to the DataTable's `ColumnSort`.
pub(crate) fn sort_dir_to_column_sort(dir: SortDir) -> gpui_component::table::ColumnSort {
    match dir {
        SortDir::Asc => gpui_component::table::ColumnSort::Ascending,
        SortDir::Desc => gpui_component::table::ColumnSort::Descending,
    }
}

// ── Transfer queue ──────────────────────────────────────────

/// Transfer direction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum TransferDirection {
    Upload,
    Download,
}

/// Transfer status.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum TransferStatus {
    InProgress,
    Completed,
    Cancelled,
    Error,
}

/// An item in the transfer queue.
#[derive(Clone)]
pub(crate) struct TransferItem {
    pub id: usize,
    pub direction: TransferDirection,
    pub filename: String,
    pub progress: f64, // 0.0 – 1.0
    pub status: TransferStatus,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use oneterm_core::RemotePath;

    use super::*;

    fn entry(name: &str, is_dir: bool, size: u64, mtime: u64) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: RemotePath::new("/d").join(name),
            is_dir,
            is_symlink: false,
            size,
            modified: Some(UNIX_EPOCH + Duration::from_secs(mtime)),
            accessed: None,
            permissions: 0o644,
            uid: Some(1000),
            gid: Some(1000),
            owner: None,
            group: None,
        }
    }

    fn names(entries: &[FileEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn format_size_picks_the_largest_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024 * 1024), "2.0 TB");
    }

    #[test]
    fn format_permissions_shows_type_special_bits_and_octal() {
        assert_eq!(format_permissions(0o100644), "-rw-r--r-- (0644)");
        assert_eq!(format_permissions(0o040755), "drwxr-xr-x (0755)");
        assert_eq!(format_permissions(0o120777), "lrwxrwxrwx (0777)");
        // setuid/setgid with execute → `s`, without → `S`.
        assert_eq!(format_permissions(0o104755), "-rwsr-xr-x (4755)");
        assert_eq!(format_permissions(0o104644), "-rwSr--r-- (4644)");
        assert_eq!(format_permissions(0o102755), "-rwxr-sr-x (2755)");
        // Sticky bit with/without other-execute → `t` / `T`.
        assert_eq!(format_permissions(0o041777), "drwxrwxrwt (1777)");
        assert_eq!(format_permissions(0o041776), "drwxrwxrwT (1776)");
        // Character device.
        assert_eq!(format_permissions(0o020666), "crw-rw-rw- (0666)");
    }

    #[test]
    fn format_owner_prefers_name_with_id() {
        assert_eq!(format_owner(Some("root"), Some(0)), "root (0)");
        assert_eq!(format_owner(Some("root"), None), "root");
        assert_eq!(format_owner(None, Some(1000)), "1000");
        assert_eq!(format_owner(None, None), "-");
    }

    #[test]
    fn format_date_handles_missing_and_epoch_values() {
        assert_eq!(format_date(None), "");
        let text = format_date(Some(UNIX_EPOCH + Duration::from_secs(86_400 * 365)));
        // Local time zone may shift the day, but the year and layout are stable.
        assert_eq!(text.len(), "YYYY-MM-DD HH:MM".len());
        assert!(text.starts_with("197"));
    }

    #[test]
    fn default_sort_is_folders_first_then_case_insensitive_name() {
        let mut entries = vec![
            entry("zeta.txt", false, 1, 1),
            entry("Beta", true, 0, 1),
            entry("alpha.txt", false, 1, 1),
            entry("alpha", true, 0, 1),
        ];
        sort_entries(&mut entries, None);
        assert_eq!(
            names(&entries),
            vec!["alpha", "Beta", "alpha.txt", "zeta.txt"]
        );
    }

    #[test]
    fn descending_sort_keeps_folders_first() {
        let mut entries = vec![
            entry("small.txt", false, 1, 1),
            entry("dir", true, 0, 1),
            entry("big.txt", false, 100, 1),
        ];
        sort_entries(&mut entries, Some((SortColumn::Size, SortDir::Desc)));
        assert_eq!(names(&entries), vec!["dir", "big.txt", "small.txt"]);

        sort_entries(&mut entries, Some((SortColumn::Name, SortDir::Desc)));
        assert_eq!(names(&entries), vec!["dir", "small.txt", "big.txt"]);
    }

    #[test]
    fn modified_sort_orders_by_timestamp() {
        let mut entries = vec![
            entry("new.txt", false, 1, 300),
            entry("old.txt", false, 1, 100),
            entry("mid.txt", false, 1, 200),
        ];
        sort_entries(&mut entries, Some((SortColumn::Modified, SortDir::Asc)));
        assert_eq!(names(&entries), vec!["old.txt", "mid.txt", "new.txt"]);
    }
}
