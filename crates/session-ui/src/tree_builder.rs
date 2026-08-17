//! Tree builder helpers — build `Vec<TreeItem>` from the session list,
//! applying the search filter + grouping + sorting.

use std::collections::BTreeMap;

use gpui::SharedString;

use gpui_component::tree::TreeItem;

use crate::session_state::{SshSession, SshSessionEntry, SshSessionId};

use super::panel::{GROUP_ID_PREFIX, SESSION_ID_PREFIX};

/// Parse the session id from a TreeItem id (`session:{id}`).
pub(crate) fn parse_session_id(id: &SharedString) -> Option<SshSessionId> {
    id.strip_prefix(SESSION_ID_PREFIX)
        .and_then(SshSessionId::parse)
}

/// The TreeItem id of a session leaf.
fn session_item_id(id: SshSessionId) -> String {
    format!("{SESSION_ID_PREFIX}{id}")
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
pub(crate) fn build_tree_items(sessions: &[SshSessionEntry], query: &str) -> Vec<TreeItem> {
    let q = query.trim().to_lowercase();

    // 1. Filter sessions if there is a query.
    let filtered: Vec<(SshSessionId, &SshSession)> = sessions
        .iter()
        .filter(|entry| q.is_empty() || session_matches(&entry.session, &q))
        .map(|entry| (entry.id, &entry.session))
        .collect();

    // 2. Split into ungrouped and grouped.
    let mut ungrouped: Vec<(SshSessionId, &SshSession)> = Vec::new();
    let mut groups: BTreeMap<String, Vec<(SshSessionId, &SshSession)>> = BTreeMap::new();

    for (id, s) in filtered {
        match &s.group {
            Some(g) if !g.trim().is_empty() => {
                groups
                    .entry(g.trim().to_string())
                    .or_default()
                    .push((id, s));
            }
            _ => {
                ungrouped.push((id, s));
            }
        }
    }

    // 3. Sort ungrouped by label.
    ungrouped.sort_by_key(|a| a.1.label.to_lowercase());

    // 4. Root items: ungrouped first, then the groups (BTreeMap is already sorted by key).
    let mut items = Vec::new();

    // Ungrouped sessions at the root.
    for (id, s) in &ungrouped {
        items.push(TreeItem::new(session_item_id(*id), s.label.clone()));
    }

    // Groups.
    for (group, mut group_sessions) in groups {
        group_sessions.sort_by_key(|a| a.1.label.to_lowercase());
        let children = group_sessions
            .iter()
            .map(|(id, s)| TreeItem::new(session_item_id(*id), s.label.clone()))
            .collect::<Vec<_>>();
        items.push(
            TreeItem::new(format!("{GROUP_ID_PREFIX}{group}"), group)
                .expanded(true)
                .children(children),
        );
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_state::SshAuthPreference;

    fn entry(id: u64, label: &str, group: Option<&str>) -> SshSessionEntry {
        let raw = serde_json::json!({
            "id": id,
            "label": label,
            "host": format!("{label}.example.test"),
            "username": if id % 2 == 0 { Some("even") } else { None },
            "group": group,
        });
        let entry: SshSessionEntry = serde_json::from_value(raw).unwrap();
        assert_eq!(entry.session.auth_method, SshAuthPreference::Password);
        entry
    }

    fn labels(items: &[TreeItem]) -> Vec<(String, String, Vec<String>)> {
        items
            .iter()
            .map(|item| {
                (
                    item.id.to_string(),
                    item.label.to_string(),
                    item.children
                        .iter()
                        .map(|child| child.id.to_string())
                        .collect(),
                )
            })
            .collect()
    }

    /// TEST-19: ungrouped first (sorted by label), then groups sorted by name
    /// with their members sorted by label; ids are the stable session ids.
    #[test]
    fn tree_groups_and_sorts_by_stable_id() {
        let sessions = vec![
            entry(10, "zulu", None),
            entry(11, "Alpha", None),
            entry(12, "web-2", Some("prod")),
            entry(13, "web-1", Some("prod")),
            entry(14, "db", Some("dev")),
            entry(15, "blank-group", Some("  ")),
        ];
        let items = build_tree_items(&sessions, "");
        assert_eq!(
            labels(&items),
            vec![
                ("session:11".into(), "Alpha".into(), vec![]),
                ("session:15".into(), "blank-group".into(), vec![]),
                ("session:10".into(), "zulu".into(), vec![]),
                ("group:dev".into(), "dev".into(), vec!["session:14".into()]),
                (
                    "group:prod".into(),
                    "prod".into(),
                    vec!["session:13".into(), "session:12".into()]
                ),
            ]
        );
        assert_eq!(
            parse_session_id(&SharedString::from("session:13")),
            Some(SshSessionId::parse("13").unwrap())
        );
        assert_eq!(parse_session_id(&SharedString::from("group:prod")), None);
        assert_eq!(
            parse_group_id(&SharedString::from("group:prod")).as_deref(),
            Some("prod")
        );
    }

    /// The filter matches label, host, username and group, case-insensitively.
    #[test]
    fn tree_filter_matches_label_host_user_and_group() {
        let sessions = vec![
            entry(1, "Alpha", None),
            entry(2, "beta", Some("Infra")),
            entry(3, "gamma", None),
        ];
        let ids = |query: &str| -> Vec<String> {
            build_tree_items(&sessions, query)
                .iter()
                .flat_map(|item| {
                    if item.children.is_empty() {
                        vec![item.id.to_string()]
                    } else {
                        item.children.iter().map(|c| c.id.to_string()).collect()
                    }
                })
                .collect()
        };
        assert_eq!(ids("ALPHA"), vec!["session:1"]);
        assert_eq!(ids("gamma.example"), vec!["session:3"]);
        assert_eq!(ids("even"), vec!["session:2"]);
        assert_eq!(ids("infra"), vec!["session:2"]);
        assert!(ids("nothing").is_empty());
    }
}
