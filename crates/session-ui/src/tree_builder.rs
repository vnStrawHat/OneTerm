//! Tree builder helpers — build `Vec<TreeItem>` from the session list,
//! applying the search filter + grouping + sorting.

use std::collections::BTreeMap;

use gpui::SharedString;

use gpui_component::tree::TreeItem;

use oneterm_state::commands::SavedSshSessionSections;

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

/// Sections for the "+" (New Terminal) menu's saved-session list (`IN-0033`):
/// the sessions with no group first, then one section per group.
///
/// Unlike [`build_tree_items`] this sorts nothing — neither the rows nor the
/// groups. Both follow the store, so the menu reads in the order
/// `ssh_session.json` holds and a group sits where it first appears.
///
/// Rows carry the session title alone: the owner asked for that during the
/// acceptance of `US-0094`, accepting that two saved sessions sharing a label
/// are then indistinguishable here (the session tree in the right dock still
/// shows their `user@host:port`). An entry whose label is blank — only a
/// hand-edited file can produce one — falls back to [`session_subtitle`]
/// rather than rendering an unidentifiable empty row.
///
/// Each row also carries the session's colour as a hex string, so the menu can
/// draw the same square the tree draws (`US-0110`). The
/// [`SshSession::DEFAULT_COLOR_HEX`] default is applied **here**, next to the
/// constant: `crates/state` only relays primitives and the menu builder in
/// `crates/terminal-view` must not learn this crate's defaults.
pub(crate) fn menu_entries(sessions: &[SshSessionEntry]) -> SavedSshSessionSections {
    let mut ungrouped: Vec<(u64, String, String)> = Vec::new();
    let mut groups: Vec<(String, Vec<(u64, String, String)>)> = Vec::new();

    for entry in sessions {
        let title = match entry.session.label.trim() {
            "" => session_subtitle(&entry.session),
            label => label.to_string(),
        };
        let color = match entry.session.color.as_deref().map(str::trim) {
            Some(hex) if !hex.is_empty() => hex.to_string(),
            _ => SshSession::DEFAULT_COLOR_HEX.to_string(),
        };
        let row = (entry.id.raw(), title, color);
        match entry.session.group.as_deref().map(str::trim) {
            Some(group) if !group.is_empty() => {
                match groups.iter_mut().find(|(name, _)| name == group) {
                    Some((_, rows)) => rows.push(row),
                    None => groups.push((group.to_string(), vec![row])),
                }
            }
            _ => ungrouped.push(row),
        }
    }

    let mut sections = Vec::with_capacity(groups.len() + 1);
    if !ungrouped.is_empty() {
        sections.push((String::new(), ungrouped));
    }
    sections.append(&mut groups);
    sections
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

    /// A `menu_entries` row for a session that saved no colour: id, title, and
    /// the default colour the producer fills in.
    fn row(id: u64, title: &str) -> (u64, String, String) {
        (
            id,
            title.to_string(),
            SshSession::DEFAULT_COLOR_HEX.to_string(),
        )
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

    #[test]
    fn item_ids_round_trip_and_reject_foreign_prefixes() {
        let session = SshSessionId::parse("42").unwrap();
        let id = SharedString::from(session_item_id(session));
        assert_eq!(parse_session_id(&id), Some(session));
        assert_eq!(parse_group_id(&id), None);

        let group = SharedString::from(format!("{GROUP_ID_PREFIX}ops"));
        assert_eq!(parse_group_id(&group), Some("ops".to_string()));
        assert_eq!(parse_session_id(&group), None);

        assert_eq!(parse_session_id(&SharedString::from("session:x")), None);
        assert_eq!(parse_session_id(&SharedString::from("other:1")), None);
    }

    #[test]
    fn subtitle_shows_user_only_when_present() {
        let mut session = entry(1, "prod", None).session;
        session.host = "10.0.0.1".into();
        session.port = 2222;
        session.username = None;
        assert_eq!(session_subtitle(&session), "10.0.0.1:2222");
        session.username = Some("root".into());
        assert_eq!(session_subtitle(&session), "root@10.0.0.1:2222");
    }

    // ── menu_entries: the "+" (New Terminal) menu rows (IN-0033) ──

    /// The ungrouped section (empty group name) holds every session in file
    /// order, titles only.
    #[test]
    fn menu_entries_lists_ungrouped_sessions_in_storage_order() {
        // Deliberately not alphabetical: the menu follows the file, unlike the tree.
        let sessions = vec![entry(7, "prod", None), entry(5, "db", None)];
        assert_eq!(
            menu_entries(&sessions),
            vec![(String::new(), vec![row(7, "prod"), row(5, "db")])]
        );
    }

    #[test]
    fn menu_entries_of_an_empty_store_is_empty() {
        assert!(menu_entries(&[]).is_empty());
    }

    /// With nothing ungrouped there is no leading empty section — the first
    /// thing under the "SSH Sessions" heading is the first group's heading.
    #[test]
    fn menu_entries_of_only_grouped_sessions_has_no_ungrouped_section() {
        let sessions = vec![
            entry(1, "db-01", Some("infra")),
            entry(2, "db-02", Some("infra")),
        ];
        assert_eq!(
            menu_entries(&sessions),
            vec![("infra".to_string(), vec![row(1, "db-01"), row(2, "db-02")])]
        );
    }

    /// Ungrouped first, then the groups in the order they first appear in the
    /// store — not sorted, which is what separates this from the tree.
    #[test]
    fn menu_entries_orders_groups_by_first_appearance() {
        let sessions = vec![
            entry(1, "prod", None),
            entry(2, "web", Some("zeta")),
            entry(3, "staging", None),
            entry(4, "db-01", Some("alpha")),
            entry(5, "cache", Some("zeta")),
        ];
        assert_eq!(
            menu_entries(&sessions),
            vec![
                (String::new(), vec![row(1, "prod"), row(3, "staging")]),
                ("zeta".to_string(), vec![row(2, "web"), row(5, "cache")]),
                ("alpha".to_string(), vec![row(4, "db-01")]),
            ],
            "groups keep store order, and a group's members do too"
        );
    }

    /// A hand-edited `"group": ""` or `"  "` is not a group: those sessions
    /// belong with the ungrouped ones, exactly as the tree treats them.
    #[test]
    fn menu_entries_treats_a_blank_group_as_ungrouped() {
        let sessions = vec![
            entry(1, "empty", Some("")),
            entry(2, "spaces", Some("  ")),
            entry(3, "none", None),
            entry(4, "real", Some(" infra ")),
        ];
        assert_eq!(
            menu_entries(&sessions),
            vec![
                (
                    String::new(),
                    vec![row(1, "empty"), row(2, "spaces"), row(3, "none")]
                ),
                ("infra".to_string(), vec![row(4, "real")]),
            ],
            "a padded group name is trimmed, a blank one is no group at all"
        );
    }

    #[test]
    fn menu_entries_falls_back_to_the_subtitle_for_a_blank_label() {
        // Only a hand-edited ssh_session.json can get here: the session dialog
        // rejects an empty label. A row with no text at all would be worse.
        let mut session = entry(3, "placeholder", None);
        session.session.label = "   ".into();
        session.session.host = "10.0.0.9".into();
        session.session.port = 2222;
        assert_eq!(
            menu_entries(&[session]),
            vec![(String::new(), vec![row(3, "10.0.0.9:2222")])]
        );
    }

    #[test]
    fn menu_entries_trims_a_padded_label() {
        let mut session = entry(4, "padded", None);
        session.session.label = "  staging  ".into();
        assert_eq!(
            menu_entries(&[session]),
            vec![(String::new(), vec![row(4, "staging")])]
        );
    }

    /// Adopted from the independent verification of `US-0094`, then narrowed by
    /// the 2026-09-15 rework: rows are the title alone, so two sessions sharing
    /// a label are deliberately identical here (the owner's call) and only the
    /// row's id tells them apart. A non-ASCII label and a long store must still
    /// map 1:1.
    #[test]
    fn verify_duplicate_unicode_and_fifty_entries() {
        let mut first = entry(1, "alpha", None);
        first.session.host = "10.9.0.1".into();
        let mut second = entry(2, "alpha", None);
        second.session.host = "10.9.0.2".into();
        let sections = menu_entries(&[first, second]);
        assert_eq!(
            sections,
            vec![(String::new(), vec![row(1, "alpha"), row(2, "alpha")])],
            "title-only rows: the id, not the text, distinguishes duplicate labels"
        );

        let unicode = entry(9, "日本-🚀", None);
        assert_eq!(
            menu_entries(&[unicode]),
            vec![(String::new(), vec![row(9, "日本-🚀")])]
        );

        let many: Vec<SshSessionEntry> = (1..=50)
            .map(|id| entry(id, &format!("host-{id:02}"), None))
            .collect();
        let sections = menu_entries(&many);
        assert_eq!(sections.len(), 1, "none of them is grouped");
        let rows = &sections[0].1;
        assert_eq!(rows.len(), 50, "every stored session gets exactly one row");
        assert_eq!(rows[0], row(1, "host-01"));
        assert_eq!(rows[49], row(50, "host-50"));
    }

    /// The saved colour reaches the menu untouched — the "+" dropdown draws the
    /// same square as the tree, from the same data (`US-0110`).
    #[test]
    fn menu_entries_carries_the_saved_colour() {
        let mut tagged = entry(1, "prod", None);
        tagged.session.color = Some("#E06C75".into());
        let mut grouped = entry(2, "db-01", Some("infra"));
        grouped.session.color = Some("#98C379".into());
        assert_eq!(
            menu_entries(&[tagged, grouped]),
            vec![
                (
                    String::new(),
                    vec![(1, "prod".to_string(), "#E06C75".to_string())]
                ),
                (
                    "infra".to_string(),
                    vec![(2, "db-01".to_string(), "#98C379".to_string())]
                ),
            ]
        );
    }

    /// A session saved before the colour tag existed, or one hand-edited to a
    /// blank value, gets the tree's default rather than an unpaintable row.
    #[test]
    fn menu_entries_applies_the_default_colour_when_none_is_saved() {
        let none = entry(1, "no-colour", None);
        assert_eq!(none.session.color, None, "the fixture saves no colour");
        let mut blank = entry(2, "blank", None);
        blank.session.color = Some("   ".into());
        assert_eq!(
            menu_entries(&[none, blank]),
            vec![(String::new(), vec![row(1, "no-colour"), row(2, "blank")])],
            "both fall back to SshSession::DEFAULT_COLOR_HEX"
        );
    }
}
