//! Catalog data model + loading.
//!
//! A catalog file is a recursive **command node** (`{ name, options,
//! subcommands? }`); see docs/auto-completion/02 §5 and 10-subcommands.md §2. The
//! embedded index [`crate::index::CATALOG_FILES`] maps every command to its JSON;
//! nodes are parsed lazily on first reference and cached
//! (docs/auto-completion/02 §5.3).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use serde::Deserialize;

use crate::family::{CatalogCategory, ShellFamily};

/// The highest catalog schema major version this build understands. Files with a
/// higher `schema` are logged and ignored (docs 02 §5).
pub const SUPPORTED_SCHEMA: u32 = 1;

/// Where a catalog entry came from — drives manual-beats-external precedence
/// (docs 02 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    External,
    Manual,
}

impl Source {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "external" => Some(Source::External),
            "manual" => Some(Source::Manual),
            _ => None,
        }
    }
}

/// A single option flag, including its trigger prefix (`/A`, `--all`, `-a`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    pub text: String,
    /// Optional short hint shown after the flag (e.g. an argument placeholder
    /// like `new-branch`, or a one-word description). Rendered italic in the UI.
    pub description: Option<String>,
}

/// A recursive command node: a name, its options, and optional subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandNode {
    pub name: String,
    pub options: Vec<Flag>,
    pub subcommands: Vec<CommandNode>,
}

impl CommandNode {
    /// Find a direct child subcommand by name (family-aware case rules).
    pub fn child(&self, name: &str, family: ShellFamily) -> Option<&CommandNode> {
        self.subcommands
            .iter()
            .find(|c| names_eq(&c.name, name, family))
    }
}

/// Family-aware name equality (case-insensitive for cmd/powershell).
pub(crate) fn names_eq(a: &str, b: &str, family: ShellFamily) -> bool {
    if family.case_insensitive() {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

// ── Raw serde forms ──────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(untagged)]
enum RawFlag {
    Str(String),
    Obj {
        flag: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl RawFlag {
    fn into_flag(self) -> Flag {
        match self {
            RawFlag::Str(text) => Flag {
                text,
                description: None,
            },
            RawFlag::Obj { flag, description } => Flag {
                text: flag,
                description: description.filter(|d| !d.trim().is_empty()),
            },
        }
    }
}

#[derive(Deserialize)]
struct RawNode {
    #[serde(default)]
    schema: Option<u32>,
    name: String,
    #[serde(default)]
    options: Vec<RawFlag>,
    #[serde(default)]
    subcommands: Vec<RawNode>,
}

impl RawNode {
    fn into_node(self) -> CommandNode {
        CommandNode {
            name: self.name,
            options: self.options.into_iter().map(RawFlag::into_flag).collect(),
            subcommands: self
                .subcommands
                .into_iter()
                .map(RawNode::into_node)
                .collect(),
        }
    }
}

/// Parse a single command node from its JSON string. Rejects unknown major
/// schema versions (returns `None`, logged by the caller). Public for tests.
pub fn parse_node(json: &str) -> Result<CommandNode, String> {
    let raw: RawNode = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if let Some(v) = raw.schema {
        if v > SUPPORTED_SCHEMA {
            return Err(format!(
                "unsupported catalog schema version {v} (max {SUPPORTED_SCHEMA})"
            ));
        }
    }
    Ok(raw.into_node())
}

// ── The catalog store ─────────────────────────────────────────────────────

struct Entry {
    name: &'static str,
    source: Source,
    category: CatalogCategory,
    json: &'static str,
}

/// The bundled command catalog: an index over the embedded files plus a lazy
/// parse cache. Lookups honor the shell's category search path and the
/// manual-beats-external precedence rule (docs 02 §6).
pub struct Catalog {
    entries: Vec<Entry>,
    cache: RefCell<HashMap<usize, Option<Rc<CommandNode>>>>,
}

impl Catalog {
    /// Build the catalog from the compile-time embedded index.
    pub fn from_embedded() -> Self {
        Self::from_raw(crate::index::CATALOG_FILES)
    }

    /// Build from an explicit `(name, source, category, json)` slice (tests).
    pub fn from_raw(raw: &[(&'static str, &'static str, &'static str, &'static str)]) -> Self {
        let mut entries = Vec::new();
        for (name, source, category, json) in raw {
            let (Some(source), Some(category)) = (
                Source::from_str(source),
                CatalogCategory::from_str(category),
            ) else {
                log::warn!(
                    "completion: skipping catalog entry {name} with unknown source/category"
                );
                continue;
            };
            entries.push(Entry {
                name,
                source,
                category,
                json,
            });
        }
        Self {
            entries,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Parse (and cache) the node for entry `idx`. A malformed file is logged and
    /// yields `None` without affecting sibling entries (fault isolation, docs 02 §3).
    fn node_at(&self, idx: usize) -> Option<Rc<CommandNode>> {
        if let Some(cached) = self.cache.borrow().get(&idx) {
            return cached.clone();
        }
        let entry = &self.entries[idx];
        let parsed = match parse_node(entry.json) {
            Ok(node) => Some(Rc::new(node)),
            Err(e) => {
                log::warn!(
                    "completion: skipping malformed catalog file {}: {e}",
                    entry.name
                );
                None
            }
        };
        self.cache.borrow_mut().insert(idx, parsed.clone());
        parsed
    }

    /// The index of the highest-precedence entry whose name matches `name`
    /// within the searched `categories`. Precedence: manual beats external
    /// (docs 02 §6); within a source the earlier category in the search order wins.
    fn best_entry(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Option<usize> {
        let mut best: Option<(usize, usize, Source)> = None; // (idx, category_rank, source)
        for (idx, entry) in self.entries.iter().enumerate() {
            if !names_eq(entry.name, name, family) {
                continue;
            }
            let Some(rank) = categories.iter().position(|c| *c == entry.category) else {
                continue;
            };
            let better = match best {
                None => true,
                Some((_, best_rank, best_source)) => {
                    // manual always beats external; else earlier category wins.
                    match (entry.source, best_source) {
                        (Source::Manual, Source::External) => true,
                        (Source::External, Source::Manual) => false,
                        _ => rank < best_rank,
                    }
                }
            };
            if better {
                best = Some((idx, rank, entry.source));
            }
        }
        best.map(|(idx, _, _)| idx)
    }

    /// Look up a top-level command node by name, honoring the category search
    /// path + precedence. Returns `None` for unknown commands.
    pub fn lookup(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Option<Rc<CommandNode>> {
        let idx = self.best_entry(name, categories, family)?;
        self.node_at(idx)
    }

    /// All distinct top-level command names available in the searched
    /// `categories`, deduped by family case-rules keeping the highest-precedence
    /// entry. Cheap: reads only the index, never parses JSON.
    pub fn command_names(
        &self,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Vec<&'static str> {
        let mut seen: Vec<&'static str> = Vec::new();
        let mut out: Vec<&'static str> = Vec::new();
        for entry in &self.entries {
            if !categories.contains(&entry.category) {
                continue;
            }
            if seen.iter().any(|s| names_eq(s, entry.name, family)) {
                continue;
            }
            seen.push(entry.name);
            // Resolve the highest-precedence spelling for display.
            if let Some(idx) = self.best_entry(entry.name, categories, family) {
                out.push(self.entries[idx].name);
            }
        }
        out
    }

    /// The source of the highest-precedence entry for `name` within
    /// `categories`, if any.
    pub fn source_of(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Option<Source> {
        self.best_entry(name, categories, family)
            .map(|idx| self.entries[idx].source)
    }

    /// Whether the highest-precedence entry for `name` came from `manual`.
    pub fn is_manual(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> bool {
        self.source_of(name, categories, family) == Some(Source::Manual)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &[(&str, &str, &str, &str)] = &[
        (
            "dir",
            "external",
            "cmd",
            r#"{ "schema": 1, "name": "dir", "options": ["/A", "/B"] }"#,
        ),
        (
            "ls",
            "external",
            "coreutils",
            r#"{ "schema": 1, "name": "ls", "options": ["-a", "--all"] }"#,
        ),
        (
            "git",
            "manual",
            "common",
            r#"{ "schema": 1, "name": "git", "options": ["-C"],
                "subcommands": [ { "name": "commit", "options": ["-m", "--amend"] } ] }"#,
        ),
        ("bad", "external", "cmd", r#"{ this is not json"#),
        (
            "future",
            "external",
            "cmd",
            r#"{ "schema": 99, "name": "future" }"#,
        ),
    ];

    fn cat() -> Catalog {
        Catalog::from_raw(RAW)
    }

    #[test]
    fn parses_string_and_object_option_forms() {
        let node = parse_node(
            r#"{ "schema": 1, "name": "grep",
                "options": ["-a", { "flag": "--all", "description": "x" }] }"#,
        )
        .unwrap();
        assert_eq!(node.options[0].text, "-a");
        assert_eq!(node.options[1].text, "--all");
    }

    #[test]
    fn rejects_unknown_major_schema() {
        assert!(parse_node(r#"{ "schema": 99, "name": "x" }"#).is_err());
    }

    #[test]
    fn lookup_resolves_by_category_path() {
        let c = cat();
        let cmd = ShellFamily::Cmd.categories(false);
        assert!(c.lookup("dir", &cmd, ShellFamily::Cmd).is_some());
        // `ls` (coreutils) is not in the cmd search path.
        assert!(c.lookup("ls", &cmd, ShellFamily::Cmd).is_none());
        let unix = ShellFamily::Unix.categories(false);
        assert!(c.lookup("ls", &unix, ShellFamily::Unix).is_some());
    }

    #[test]
    fn lookup_is_case_insensitive_for_cmd_only() {
        let c = cat();
        let cmd = ShellFamily::Cmd.categories(false);
        assert!(c.lookup("DIR", &cmd, ShellFamily::Cmd).is_some());
        let unix = ShellFamily::Unix.categories(false);
        // Unix is case-sensitive: "LS" must not resolve to "ls".
        assert!(c.lookup("LS", &unix, ShellFamily::Unix).is_none());
    }

    #[test]
    fn subcommand_child_lookup() {
        let c = cat();
        let common = vec![CatalogCategory::Common];
        let git = c.lookup("git", &common, ShellFamily::Unix).unwrap();
        let commit = git.child("commit", ShellFamily::Unix).unwrap();
        assert_eq!(commit.options.len(), 2);
    }

    #[test]
    fn malformed_file_skipped_without_breaking_siblings() {
        let c = cat();
        let cmd = ShellFamily::Cmd.categories(false);
        // "bad" (malformed) and "future" (unknown schema) yield None…
        assert!(c.lookup("bad", &cmd, ShellFamily::Cmd).is_none());
        assert!(c.lookup("future", &cmd, ShellFamily::Cmd).is_none());
        // …but "dir" still resolves.
        assert!(c.lookup("dir", &cmd, ShellFamily::Cmd).is_some());
    }

    #[test]
    fn command_names_deduped_across_categories() {
        let c = cat();
        let unix = ShellFamily::Unix.categories(false);
        let names = c.command_names(&unix, ShellFamily::Unix);
        assert!(names.contains(&"ls"));
        assert!(names.contains(&"git"));
        assert!(!names.contains(&"dir")); // cmd-only, not in unix path
    }
}
