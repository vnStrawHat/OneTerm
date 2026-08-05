//! Tree builder helpers — build `Vec<TreeItem>` from the session list,
//! applying the search filter + grouping + sorting.

use std::collections::BTreeMap;

use gpui::SharedString;

use gpui_component::tree::TreeItem;

use crate::session_state::SshSession;

use super::panel::{GROUP_ID_PREFIX, SESSION_ID_PREFIX};

/// Parse the store index from a TreeItem id (`session:{ix}`).
pub(crate) fn parse_session_id(id: &SharedString) -> Option<usize> {
    id.strip_prefix(SESSION_ID_PREFIX)
        .and_then(|s| s.parse::<usize>().ok())
}

/// Parse the group name from a TreeItem id (`group:{name}`).
pub(crate) fn parse_group_id(id: &SharedString) -> Option<String> {
    id.strip_prefix(GROUP_ID_PREFIX).map(|s| s.to_string())
}

/// Build the subtitle for a session leaf: `user@host:port` or `host:port`.
pub(crate) fn session_subtitle(s: &SshSession) -> String {
    match &s.username {
        Some(u) => format!("{}@{}:{}", u, s.host, s.port),
        None => format!("{}:{}", s.host, s.port),
    }
}

/// Check whether a session matches the search query (case-insensitive).
///
/// Matches on: label, host, username, group name.
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

/// Build `Vec<TreeItem>` from the session list — applies search filter + grouping + sorting.
///
/// - Empty `query` → show everything.
/// - Non-empty `query` → show only matching sessions (label/host/user/group).
/// - Items without a group → root (on top), sorted by label.
/// - Items with a group → a folder per group name (sorted), sorted by label within the folder.
pub(crate) fn build_tree_items(sessions: &[SshSession], query: &str) -> Vec<TreeItem> {
    let q = query.trim().to_lowercase();

    // 1. Filter sessions if there is a query.
    let filtered: Vec<(usize, &SshSession)> = if q.is_empty() {
        sessions.iter().enumerate().collect()
    } else {
        sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| session_matches(s, &q))
            .collect()
    };

    // 2. Split into ungrouped and grouped.
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

    // 3. Sort ungrouped by label.
    ungrouped.sort_by_key(|a| a.1.label.to_lowercase());

    // 4. Root items: ungrouped first, then the groups (BTreeMap is already sorted by key).
    let mut items = Vec::new();

    // Ungrouped sessions at the root.
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
