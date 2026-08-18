//! Catalog data model + loading.
//!
//! A catalog file is a recursive **command node** (`{ name, options,
//! subcommands? }`); see docs/auto-completion/02 §5 and 10-subcommands.md §2.
//! `build.rs` walks `assets/**/*.json` and generates `catalog_index.rs`, which
//! defines `CATALOG_FILES` as `&[(name, source, category, json)]` where each
//! `json` is an `include_str!` of the file (docs/auto-completion/07 §5); nodes
//! are parsed lazily on first reference and cached (docs/auto-completion/02 §5.3).

include!(concat!(env!("OUT_DIR"), "/catalog_index.rs"));

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::family::{CatalogCategory, ShellFamily};

#[path = "catalog_schema.rs"]
mod schema;
#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

use schema::parse_node;

/// Where a catalog entry came from — drives manual-beats-external precedence
/// (docs 02 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
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
pub(crate) struct Flag {
    pub text: String,
    /// Optional short hint shown after the flag (e.g. an argument placeholder
    /// like `new-branch`, or a one-word description). Rendered italic in the UI.
    pub description: Option<String>,
}

/// A recursive command node: a name, its options, and optional subcommands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandNode {
    pub name: String,
    pub options: Vec<Flag>,
    pub subcommands: Vec<CommandNode>,
}

impl CommandNode {
    /// Find a direct child subcommand by name (family-aware case rules).
    pub(crate) fn child(&self, name: &str, family: ShellFamily) -> Option<&CommandNode> {
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
pub(crate) struct Catalog {
    entries: Vec<Entry>,
    cache: RefCell<HashMap<usize, Option<Rc<CommandNode>>>>,
}

impl Catalog {
    /// Build the catalog from the compile-time embedded index.
    pub(crate) fn from_embedded() -> Self {
        Self::from_raw(CATALOG_FILES)
    }

    /// Build from an explicit `(name, source, category, json)` slice (tests).
    pub(crate) fn from_raw(
        raw: &[(&'static str, &'static str, &'static str, &'static str)],
    ) -> Self {
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
    pub(crate) fn lookup(
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
    /// entry (same rule as [`Self::best_entry`]), in first-seen order. One pass
    /// over the index (called per keystroke — PERF-24); never parses JSON.
    pub(crate) fn command_names(
        &self,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Vec<&'static str> {
        // normalized name → (position in `order`, category rank, source)
        let mut best: HashMap<String, (usize, usize, Source)> = HashMap::new();
        let mut order: Vec<&'static str> = Vec::new();
        for entry in &self.entries {
            let Some(rank) = categories.iter().position(|c| *c == entry.category) else {
                continue;
            };
            let key = if family.case_insensitive() {
                entry.name.to_ascii_lowercase()
            } else {
                entry.name.to_string()
            };
            match best.get_mut(&key) {
                None => {
                    best.insert(key, (order.len(), rank, entry.source));
                    order.push(entry.name);
                }
                Some((position, best_rank, best_source)) => {
                    let better = match (entry.source, *best_source) {
                        (Source::Manual, Source::External) => true,
                        (Source::External, Source::Manual) => false,
                        _ => rank < *best_rank,
                    };
                    if better {
                        *best_rank = rank;
                        *best_source = entry.source;
                        order[*position] = entry.name;
                    }
                }
            }
        }
        order
    }

    /// The source of the highest-precedence entry for `name` within
    /// `categories`, if any.
    fn source_of(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> Option<Source> {
        self.best_entry(name, categories, family)
            .map(|idx| self.entries[idx].source)
    }

    /// Whether the highest-precedence entry for `name` came from `manual`.
    pub(crate) fn is_manual(
        &self,
        name: &str,
        categories: &[CatalogCategory],
        family: ShellFamily,
    ) -> bool {
        self.source_of(name, categories, family) == Some(Source::Manual)
    }
}
