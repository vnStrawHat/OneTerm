//! Types + helpers for the SFTP browser — sort state, transfer queue,
//! column definitions, formatting.

use std::cmp::Reverse;
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
    match (col, dir) {
        (SortColumn::Name, SortDir::Asc) => {
            entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()))
        }
        (SortColumn::Name, SortDir::Desc) => {
            entries.sort_by_cached_key(|e| (!e.is_dir, Reverse(e.name.to_lowercase())))
        }
        (SortColumn::Modified, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.modified)),
        (SortColumn::Modified, SortDir::Desc) => {
            entries.sort_by_key(|e| (!e.is_dir, Reverse(e.modified)))
        }
        (SortColumn::Size, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.size)),
        (SortColumn::Size, SortDir::Desc) => entries.sort_by_key(|e| (!e.is_dir, Reverse(e.size))),
        (SortColumn::Permissions, SortDir::Asc) => {
            entries.sort_by_key(|e| (!e.is_dir, e.permissions))
        }
        (SortColumn::Permissions, SortDir::Desc) => {
            entries.sort_by_key(|e| (!e.is_dir, Reverse(e.permissions)))
        }
        (SortColumn::Owner, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.uid)),
        (SortColumn::Owner, SortDir::Desc) => entries.sort_by_key(|e| (!e.is_dir, Reverse(e.uid))),
        (SortColumn::Group, SortDir::Asc) => entries.sort_by_key(|e| (!e.is_dir, e.gid)),
        (SortColumn::Group, SortDir::Desc) => entries.sort_by_key(|e| (!e.is_dir, Reverse(e.gid))),
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

/// Column resize limits (px), the same for every resizable column.
pub(crate) const COLUMN_MIN_WIDTH: f32 = 40.0;
pub(crate) const COLUMN_MAX_WIDTH: f32 = 800.0;

/// Natural widths of the two fixed columns of the default set, shared by the
/// remote and the local table so the two panes line up.
pub(crate) const SIZE_COLUMN_WIDTH: f32 = 72.0;
pub(crate) const MODIFIED_COLUMN_WIDTH: f32 = 116.0;

/// Name never shrinks below this, even when the panel is too narrow for the
/// whole set — below that the table scrolls sideways, which is the honest
/// answer to "more columns than fit".
pub(crate) const NAME_COLUMN_MIN_WIDTH: f32 = 100.0;

/// What the table needs beyond the sum of its columns: `render_last_empty_col`
/// (`w_3`) plus the cell border.
const TABLE_TRAILING_GUTTER: f32 = 16.0;

/// Width of the Name column: the panel width the other visible columns do not
/// take (US-0124 / F29).
///
/// Name is the flexible column of both tables. That is what keeps the docked
/// browser free of a horizontal scrollbar at its shipped width, and what keeps
/// a wide dual pane free of the blank strip that used to look like an extra,
/// always-empty column.
pub(crate) fn name_column_width(available_width: f32, other_columns_width: f32) -> f32 {
    (available_width - other_columns_width - TABLE_TRAILING_GUTTER).max(NAME_COLUMN_MIN_WIDTH)
}

/// Definition of a column in the file list — display config + resize/visibility
/// state (persisted to `docks.json`).
#[derive(Clone, Debug)]
pub(crate) struct SftpColumnConfig {
    pub col: SortColumn,
    /// Header label.
    pub label: &'static str,
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
            col,
            label,
            right_align,
            visible: true,
            width: default_width,
        }
    }

    fn shown(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

/// Canonical column list (order left → right). Name is always visible.
///
/// Only Name, Size and Date Modified are shown by default: they are what fits
/// the panel the browser ships in (US-0124 / F29). Permissions, Owner and Group
/// stay one click away in the toolbar menu's Columns section, and the choice is
/// persisted. Name's width is derived from the panel width
/// ([`name_column_width`]), so its configured width is only the fallback used
/// before the first measurement.
pub(crate) fn default_column_configs() -> Vec<SftpColumnConfig> {
    vec![
        // Name starts at its floor: the first frame is drawn before the panel
        // has been measured, and a wider guess would overflow a narrow dock for
        // that one frame (m5).
        SftpColumnConfig::new(SortColumn::Name, "Name", NAME_COLUMN_MIN_WIDTH, false).shown(true),
        SftpColumnConfig::new(SortColumn::Size, "Size", SIZE_COLUMN_WIDTH, true).shown(true),
        SftpColumnConfig::new(
            SortColumn::Modified,
            "Date Modified",
            MODIFIED_COLUMN_WIDTH,
            false,
        )
        .shown(true),
        SftpColumnConfig::new(SortColumn::Permissions, "Permissions", 150.0, false).shown(false),
        SftpColumnConfig::new(SortColumn::Owner, "Owner", 90.0, false).shown(false),
        SftpColumnConfig::new(SortColumn::Group, "Group", 90.0, false).shown(false),
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

    /// `oneterm-state` migrates the persisted column map and cannot see
    /// `SortColumn`, so it works from `SFTP_TABLE_COLUMNS`. The two must name
    /// the same columns or the migration would drop a live column's setting.
    #[test]
    fn the_persisted_column_keys_match_the_columns() {
        let mut from_columns: Vec<&str> = default_column_configs()
            .iter()
            .map(|cfg| cfg.col.key())
            .collect();
        let mut shared: Vec<&str> = oneterm_core::SFTP_TABLE_COLUMNS.to_vec();
        from_columns.sort_unstable();
        shared.sort_unstable();
        assert_eq!(from_columns, shared);
    }

    /// F29: the docked panel is ~317 px after US-0113 at a 900 px window and
    /// ~420-490 px at wider ones. The default set has to fit all of those
    /// without a horizontal scrollbar, which means Name absorbs what is left.
    #[test]
    fn the_default_columns_fit_the_docked_panel() {
        let others = SIZE_COLUMN_WIDTH + MODIFIED_COLUMN_WIDTH;
        for available in [317.0, 420.0, 490.0, 1900.0] {
            let name = name_column_width(available, others);
            assert!(
                name + others + TABLE_TRAILING_GUTTER <= available,
                "the default set overflows a {available} px panel"
            );
            assert!(name >= NAME_COLUMN_MIN_WIDTH);
        }
        // Wider panel, wider Name: no blank strip masquerading as a column.
        assert!(name_column_width(1900.0, others) > name_column_width(490.0, others));
        // Default visibility is exactly Name + Size + Date Modified.
        let shown: Vec<&str> = default_column_configs()
            .iter()
            .filter(|c| c.visible)
            .map(|c| c.col.key())
            .collect();
        assert_eq!(shown, vec!["name", "size", "modified"]);
    }

    /// Too narrow for the set: Name stops at its minimum and the table scrolls
    /// rather than squeezing names into nothing.
    #[test]
    fn name_stops_shrinking_at_its_minimum() {
        assert_eq!(name_column_width(120.0, 188.0), NAME_COLUMN_MIN_WIDTH);
        assert_eq!(name_column_width(0.0, 0.0), NAME_COLUMN_MIN_WIDTH);
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
