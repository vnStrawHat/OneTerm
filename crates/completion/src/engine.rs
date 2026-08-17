//! The suggestion engine — a pure function of its inputs (docs 04 §1).
//!
//! `Engine::suggest(history, ctx, params)` parses the input line, resolves the
//! command/subcommand tree, gathers candidates from the enabled sources, matches
//! + ranks + dedups them, and returns a bounded, redaction-safe suggestion list.
//!
//! Per-context candidate gathering + finalization live in [`gather`]; the
//! matching and ranking helpers live in [`scoring`].

use std::rc::Rc;

use crate::catalog::{Catalog, CommandNode, Flag};
use crate::family::ShellFamily;
use crate::history::CompletionHistory;
use crate::params::CompletionParams;
use crate::parse::ParsedLine;

mod gather;
mod resolve;
mod scoring;
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

use scoring::match_token;

/// The kind of a suggestion → drives its tag badge + color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    History,
    Command,
    Option,
}

impl SuggestionKind {
    /// The single-letter tag badge (docs 01 §5, 05 §5).
    pub fn tag(self) -> char {
        match self {
            SuggestionKind::History => 'H',
            SuggestionKind::Command => 'C',
            SuggestionKind::Option => 'O',
        }
    }
}

/// One candidate returned to the UI.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// The full text to display / the completion target.
    pub text: String,
    pub kind: SuggestionKind,
    /// Optional short hint shown after the text (argument placeholder / one-word
    /// description), rendered italic in the overlay. Only options carry one.
    pub description: Option<String>,
    /// Matched-prefix length (for highlight), measured from the start of `text`.
    pub match_len: usize,
    /// Ranking score (higher = better).
    pub score: f32,
    /// Byte offset in the input line where the typed portion this suggestion
    /// replaces begins — `token_start` for token suggestions, `0` for whole-line
    /// history recall. The controller appends `text[len(line[replace_from..cursor])..]`.
    pub replace_from: usize,
}

impl Suggestion {
    /// Whether accepting this suggestion is a pure prefix extension of `typed`.
    ///
    /// `typed.len()` may fall inside a multibyte char of `text` (fuzzy match
    /// on a history-derived candidate such as `日x` with typed `x`), so the
    /// slice is checked with `get` rather than indexed.
    pub fn is_prefix_of_typed(&self, typed: &str) -> bool {
        self.text
            .get(..typed.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(typed))
    }
}

/// The query the UI hands the engine each keystroke.
#[derive(Debug, Clone)]
pub struct CompletionContext<'a> {
    pub family: ShellFamily,
    /// The full input line (prompt-relative).
    pub line: &'a str,
    /// Byte offset of the cursor within `line`.
    pub cursor_col: usize,
    /// Current time in ms, for frecency (engine stays clock-free — docs 04 §1).
    pub now_ms: u64,
}

/// The resolved command tree position (docs 10 §3).
#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    /// The active command node — shared with the catalog's lazy parse cache, so
    /// resolution never deep-clones the root subtree.
    pub active: Rc<CommandNode>,
    /// Options of the ancestors of `active` (ranked below active's own).
    pub ancestor_options: Vec<Flag>,
}

/// The suggestion engine (lazily-parsed embedded catalogs).
pub struct Engine {
    catalog: Catalog,
}
// Shared inputs bundled to keep the per-context gather helpers tidy.
struct Query<'a> {
    history: &'a CompletionHistory,
    ctx: &'a CompletionContext<'a>,
    cfg: &'a CompletionParams,
    family: ShellFamily,
}

// Internal candidate carrying ranking metadata before conversion to `Suggestion`.
struct Candidate {
    text: String,
    kind: SuggestionKind,
    description: Option<String>,
    is_manual: bool,
    is_prefix: bool,
    match_len: usize,
    frecency: f32,
    replace_from: usize,
}

impl Engine {
    /// Build the engine from the compile-time embedded catalog index.
    pub fn from_embedded() -> Self {
        Self {
            catalog: Catalog::from_embedded(),
        }
    }

    /// Build the engine from an explicit catalog (tests).
    #[cfg(test)]
    pub(crate) fn with_catalog(catalog: Catalog) -> Self {
        Self { catalog }
    }

    /// Produce a ranked, deduped, redaction-safe suggestion list.
    pub fn suggest(
        &self,
        history: &CompletionHistory,
        ctx: &CompletionContext,
        cfg: &CompletionParams,
    ) -> Vec<Suggestion> {
        let family = ctx.family;
        let cursor = ctx.cursor_col.min(ctx.line.len());
        let p = ParsedLine::parse(ctx.line, cursor);
        let ci = family.case_insensitive();

        let mut candidates: Vec<Candidate> = Vec::new();
        let q = Query {
            history,
            ctx,
            cfg,
            family,
        };

        let token = p.token.clone();
        let is_option = !token.is_empty() && family.is_option_token(&token);

        if is_option && p.head.is_some() {
            // ── Option context ────────────────────────────────────────────
            self.gather_options(&q, &p, &token, &mut candidates);
        } else if p.is_first_token {
            // ── Command context ───────────────────────────────────────────
            if token.len() < cfg.min_prefix_len && !(cfg.suggest_on_empty && token.is_empty()) {
                return Vec::new();
            }
            self.gather_commands(&q, &p, &token, &mut candidates);
        } else {
            // Resolve the tree to decide subcommand vs argument.
            let resolved = self.resolve(&p, family, cfg.windows_allow_coreutils);
            let has_children = resolved
                .as_ref()
                .map(|r| !r.active.subcommands.is_empty())
                .unwrap_or(false);
            if has_children {
                // ── Subcommand context ────────────────────────────────────
                if let Some(r) = &resolved {
                    for child in &r.active.subcommands {
                        if let Some((is_prefix, mlen)) = match_token(&child.name, &token, ci, cfg) {
                            candidates.push(Candidate {
                                text: child.name.clone(),
                                description: None,
                                kind: SuggestionKind::Command,
                                is_manual: true,
                                is_prefix,
                                match_len: mlen,
                                frecency: 0.0,
                                replace_from: p.token_start,
                            });
                        }
                    }
                }
                self.gather_history_whole_line(&q, &mut candidates);
            } else {
                // ── Argument context (history-only) ───────────────────────
                self.gather_history_whole_line(&q, &mut candidates);
            }
        }

        self.finalize(candidates, family, cfg)
    }
}
