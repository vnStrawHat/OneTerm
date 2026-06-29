//! Tree builder helpers — build `Vec<TreeItem>` từ danh sách session,
//! áp dụng search filter + grouping + sorting.
//!
//! Tách từ `tabs.rs` để giảm độ dài file.

use std::collections::BTreeMap;

use gpui::SharedString;

use gpui_component::tree::TreeItem;

use crate::state::SshSession;

use super::panel::{GROUP_ID_PREFIX, SESSION_ID_PREFIX};

/// Parse store index từ TreeItem id (`session:{ix}`).
pub(crate) fn parse_session_id(id: &SharedString) -> Option<usize> {
    id.strip_prefix(SESSION_ID_PREFIX)
        .and_then(|s| s.parse::<usize>().ok())
}

/// Parse group name từ TreeItem id (`group:{name}`).
pub(crate) fn parse_group_id(id: &SharedString) -> Option<String> {
    id.strip_prefix(GROUP_ID_PREFIX).map(|s| s.to_string())
}

/// Tạo subtitle cho session leaf: `user@host:port` hoặc `host:port`.
pub(crate) fn session_subtitle(s: &SshSession) -> String {
    match &s.username {
        Some(u) => format!("{}@{}:{}", u, s.host, s.port),
        None => format!("{}:{}", s.host, s.port),
    }
}

/// Kiểm tra session có khớp với search query (case-insensitive).
///
/// Match trên: label, host, username, group name.
pub(crate) fn session_matches(s: &SshSession, q: &str) -> bool {
    s.label.to_lowercase().contains(q)
        || s.host.to_lowercase().contains(q)
        || s.username
            .as_ref()
            .map(|u| u.to_lowercase().contains(q))
            .unwrap_or(false)
        || s.group
            .as_ref()
            .map(|g| g.trim().to_lowercase().contains(q))
            .unwrap_or(false)
}

/// Build `Vec<TreeItem>` từ danh sách session — áp dụng search filter + grouping + sorting.
///
/// - `query` rỗng → hiển thị tất cả.
/// - `query` không rỗng → chỉ hiển thị session khớp (label/host/user/group).
/// - Item không có group → root (trên cùng), sort theo label.
/// - Item có group → folder theo group name (sort), trong folder sort theo label.
pub(crate) fn build_tree_items(sessions: &[SshSession], query: &str) -> Vec<TreeItem> {
    let q = query.trim().to_lowercase();

    // 1. Filter sessions nếu có query.
    let filtered: Vec<(usize, &SshSession)> = if q.is_empty() {
        sessions.iter().enumerate().collect()
    } else {
        sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| session_matches(s, &q))
            .collect()
    };

    // 2. Tách ungrouped và grouped.
    let mut ungrouped: Vec<(usize, &SshSession)> = Vec::new();
    let mut groups: BTreeMap<String, Vec<(usize, &SshSession)>> = BTreeMap::new();

    for (ix, s) in filtered {
        match &s.group {
            Some(g) if !g.trim().is_empty() => {
                groups
                    .entry(g.trim().to_string())
                    .or_default()
                    .push((ix, s));
            }
            _ => {
                ungrouped.push((ix, s));
            }
        }
    }

    // 3. Sort ungrouped theo label.
    ungrouped.sort_by_key(|a| a.1.label.to_lowercase());

    // 4. Root items: ungrouped trước, rồi đến groups (BTreeMap đã sort theo key).
    let mut items = Vec::new();

    // Ungrouped sessions ở root.
    for (ix, s) in &ungrouped {
        items.push(TreeItem::new(
            format!("{SESSION_ID_PREFIX}{ix}"),
            s.label.clone(),
        ));
    }

    // Groups.
    for (group, mut group_sessions) in groups {
        group_sessions.sort_by_key(|a| a.1.label.to_lowercase());
        let children = group_sessions
            .iter()
            .map(|(ix, s)| TreeItem::new(format!("{SESSION_ID_PREFIX}{ix}"), s.label.clone()))
            .collect::<Vec<_>>();
        items.push(
            TreeItem::new(format!("{GROUP_ID_PREFIX}{group}"), group)
                .expanded(true)
                .children(children),
        );
    }

    items
}
