//! The suggestion engine — a pure function of its inputs (docs 04 §1).
//!
//! `Engine::suggest(history, ctx, params)` parses the input line, resolves the
//! command/subcommand tree, gathers candidates from the enabled sources, matches
//! + ranks + dedups them, and returns a bounded, redaction-safe suggestion list.

use crate::catalog::{Catalog, CommandNode, Flag, names_eq};
use crate::family::ShellFamily;
use crate::history::{CompletionHistory, prefix_match};
use crate::params::CompletionParams;
use crate::parse::ParsedLine;
use crate::redact;

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
    /// Byte offset within `text` where the matched prefix begins (0 for prefix).
    pub match_start: usize,
    /// Matched-prefix length (for highlight).
    pub match_len: usize,
    /// Ranking score (higher = better).
    pub score: f32,
    /// Byte offset in the input line where the typed portion this suggestion
    /// replaces begins — `token_start` for token suggestions, `0` for whole-line
    /// history recall. The controller appends `text[len(line[replace_from..cursor])..]`.
    pub replace_from: usize,
}

impl Suggestion {
    /// The bytes to append to the PTY given the text the user already typed
    /// (append-only, docs 04 §5). Empty if `text` is not a prefix extension of
    /// `typed` (a fuzzy match — accept is gated by `allow_fuzzy_accept`).
    pub fn remainder<'a>(&'a self, typed: &str) -> &'a str {
        if typed.len() > self.text.len() {
            return "";
        }
        let head = &self.text[..typed.len()];
        if head.eq_ignore_ascii_case(typed) {
            &self.text[typed.len()..]
        } else {
            ""
        }
    }

    /// Whether accepting this suggestion is a pure prefix extension of `typed`.
    pub fn is_prefix_of_typed(&self, typed: &str) -> bool {
        typed.len() <= self.text.len() && self.text[..typed.len()].eq_ignore_ascii_case(typed)
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
pub struct Resolved {
    pub active: CommandNode,
    /// Command path from root to active (breadcrumb — docs 10 §5).
    pub path_names: Vec<String>,
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
    match_start: usize,
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
    pub fn with_catalog(catalog: Catalog) -> Self {
        Self { catalog }
    }

    /// Resolve the active command node + path from the tokens left of the cursor
    /// (docs 10 §3). Returns `None` for an unknown top-level command.
    pub fn resolve(
        &self,
        p: &ParsedLine,
        family: ShellFamily,
        allow_coreutils: bool,
    ) -> Option<Resolved> {
        let categories = family.categories(allow_coreutils);
        let head = p.head.as_deref()?;
        let root = self.catalog.lookup(head, &categories, family)?;
        let mut active: CommandNode = (*root).clone();
        let mut path_names = vec![active.name.clone()];
        let mut ancestor_options: Vec<Flag> = Vec::new();
        for tok in p.prior_tokens.iter().skip(1) {
            if family.is_option_token(tok) {
                continue;
            }
            if let Some(child) = active.child(tok, family) {
                let child = child.clone();
                ancestor_options.extend(active.options.clone());
                active = child;
                path_names.push(active.name.clone());
            } else {
                break;
            }
        }
        Some(Resolved {
            active,
            path_names,
            ancestor_options,
        })
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
                                match_start: 0,
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

    fn gather_commands(&self, q: &Query, p: &ParsedLine, token: &str, out: &mut Vec<Candidate>) {
        let (cfg, family) = (q.cfg, q.family);
        let ci = family.case_insensitive();
        if cfg.sources.external || cfg.sources.manual {
            let categories = family.categories(cfg.windows_allow_coreutils);
            for name in self.catalog.command_names(&categories, family) {
                if let Some((is_prefix, mlen)) = match_token(name, token, ci, cfg) {
                    out.push(Candidate {
                        text: name.to_string(),
                        description: None,
                        kind: SuggestionKind::Command,
                        is_manual: self.catalog.is_manual(name, &categories, family),
                        is_prefix,
                        match_start: 0,
                        match_len: mlen,
                        frecency: 0.0,
                        replace_from: p.token_start,
                    });
                }
            }
        }
        // History first tokens.
        if cfg.sources.memory {
            for rec in q.history.entries(family) {
                let first = rec.line.split_whitespace().next().unwrap_or("");
                if first.is_empty() {
                    continue;
                }
                if let Some((is_prefix, mlen)) = match_token(first, token, ci, cfg) {
                    out.push(Candidate {
                        text: first.to_string(),
                        description: None,
                        kind: SuggestionKind::History,
                        is_manual: false,
                        is_prefix,
                        match_start: 0,
                        match_len: mlen,
                        frecency: rec.frecency(q.ctx.now_ms),
                        replace_from: p.token_start,
                    });
                }
            }
        }
    }

    fn gather_options(&self, q: &Query, p: &ParsedLine, token: &str, out: &mut Vec<Candidate>) {
        let (cfg, family) = (q.cfg, q.family);
        let resolved = self.resolve(p, family, cfg.windows_allow_coreutils);
        if let Some(r) = resolved {
            if cfg.sources.manual || cfg.sources.external {
                for flag in &r.active.options {
                    if let Some((is_prefix, mlen)) = match_option(&flag.text, token, cfg) {
                        out.push(Candidate {
                            text: flag.text.clone(),
                            kind: SuggestionKind::Option,
                            description: flag.description.clone(),
                            is_manual: true,
                            is_prefix,
                            match_start: 0,
                            match_len: mlen,
                            frecency: 0.0,
                            replace_from: p.token_start,
                        });
                    }
                }
                if cfg.inherit_ancestor_options {
                    for flag in &r.ancestor_options {
                        if let Some((is_prefix, mlen)) = match_option(&flag.text, token, cfg) {
                            out.push(Candidate {
                                text: flag.text.clone(),
                                kind: SuggestionKind::Option,
                                description: flag.description.clone(),
                                is_manual: true,
                                is_prefix,
                                match_start: 0,
                                match_len: mlen,
                                // Rank ancestor options below the active node's own.
                                frecency: -1.0,
                                replace_from: p.token_start,
                            });
                        }
                    }
                }
            }
        } else if cfg.sources.memory {
            // Unknown command → history-derived options (docs 04 §3.2).
            if let Some(head) = p.head.as_deref() {
                let mut seen: Vec<String> = Vec::new();
                for rec in q.history.entries(family) {
                    let mut toks = rec.line.split_whitespace();
                    if !toks.next().is_some_and(|h| names_eq(h, head, family)) {
                        continue;
                    }
                    for t in toks {
                        if family.is_option_token(t) && !seen.iter().any(|s| s == t) {
                            if let Some((is_prefix, mlen)) = match_option(t, token, cfg) {
                                seen.push(t.to_string());
                                out.push(Candidate {
                                    text: t.to_string(),
                                    description: None,
                                    kind: SuggestionKind::History,
                                    is_manual: false,
                                    is_prefix,
                                    match_start: 0,
                                    match_len: mlen,
                                    frecency: rec.frecency(q.ctx.now_ms),
                                    replace_from: p.token_start,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn gather_history_whole_line(&self, q: &Query, out: &mut Vec<Candidate>) {
        let (cfg, family, ctx) = (q.cfg, q.family, q.ctx);
        if !cfg.sources.memory {
            return;
        }
        let ci = family.case_insensitive();
        let cursor = ctx.cursor_col.min(ctx.line.len());
        let line_prefix = ctx.line[..cursor].trim_start();
        for rec in q.history.entries(family) {
            if rec.line.len() <= line_prefix.len() {
                continue; // no remainder to add
            }
            if prefix_match(&rec.line, line_prefix, ci) {
                out.push(Candidate {
                    text: rec.line.clone(),
                    description: None,
                    kind: SuggestionKind::History,
                    is_manual: false,
                    is_prefix: true,
                    match_start: 0,
                    match_len: line_prefix.len(),
                    frecency: rec.frecency(ctx.now_ms),
                    replace_from: 0,
                });
            }
        }
    }

    /// Score, dedup, drop secrets, sort, and truncate.
    fn finalize(
        &self,
        candidates: Vec<Candidate>,
        family: ShellFamily,
        cfg: &CompletionParams,
    ) -> Vec<Suggestion> {
        let ci = family.case_insensitive();
        // Score each candidate.
        let mut scored: Vec<(f32, Candidate)> = candidates
            .into_iter()
            .filter(|c| {
                // Suggestion-time secret guard (defense in depth, docs 08 §5).
                !(cfg.redact_sensitive
                    && c.kind == SuggestionKind::History
                    && redact::contains_secret(&c.text))
            })
            .map(|c| (score(&c, cfg), c))
            .collect();

        // Sort by score desc so dedup keeps the best-scoring duplicate.
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Dedup by normalized text, keeping the highest-precedence tag + score.
        let mut result: Vec<(f32, Candidate)> = Vec::new();
        for (s, c) in scored {
            if let Some((_, existing)) = result
                .iter_mut()
                .find(|(_, e)| text_eq(&e.text, &c.text, ci))
            {
                // Keep the higher-precedence tag (H > manual > external).
                if precedence(&c) > precedence(existing) {
                    existing.kind = c.kind;
                    existing.is_manual = c.is_manual;
                    existing.description = c.description;
                }
                continue;
            }
            result.push((s, c));
        }

        // Already sorted by score; take a bounded window.
        result.truncate(cfg.hard_cap());
        result
            .into_iter()
            .map(|(score, c)| Suggestion {
                text: c.text,
                kind: c.kind,
                description: c.description,
                match_start: c.match_start,
                match_len: c.match_len,
                score,
                replace_from: c.replace_from,
            })
            .collect()
    }
}

/// Precedence for dedup tag-keeping: History > manual C/O > external C/O.
fn precedence(c: &Candidate) -> u8 {
    match c.kind {
        SuggestionKind::History => 3,
        _ if c.is_manual => 2,
        _ => 1,
    }
}

/// Family-aware text equality for dedup.
fn text_eq(a: &str, b: &str, ci: bool) -> bool {
    if ci {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Match a command/subcommand name against a token: prefix first, then fuzzy.
/// Returns `(is_prefix, match_len)` or `None`.
fn match_token(name: &str, token: &str, ci: bool, cfg: &CompletionParams) -> Option<(bool, usize)> {
    if token.is_empty() {
        return Some((true, 0));
    }
    if prefix_match(name, token, ci) {
        return Some((true, token.len()));
    }
    if cfg.fuzzy && is_subsequence(name, token, ci) {
        return Some((false, 0));
    }
    None
}

/// Match an option flag against a token (prefix, including its trigger).
fn match_option(flag: &str, token: &str, cfg: &CompletionParams) -> Option<(bool, usize)> {
    // Options are matched case-insensitively (Windows `/Q` vs `/q`); Unix flags
    // are already distinct enough that this does not misfire in practice.
    if token.is_empty() {
        return Some((true, 0));
    }
    if flag.len() >= token.len() && flag[..token.len()].eq_ignore_ascii_case(token) {
        return Some((true, token.len()));
    }
    if cfg.fuzzy && is_subsequence(flag, token, true) {
        return Some((false, 0));
    }
    None
}

/// Whether `needle` is a subsequence of `haystack` (fuzzy match).
fn is_subsequence(haystack: &str, needle: &str, ci: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    let hay: Vec<char> = if ci {
        haystack.to_ascii_lowercase().chars().collect()
    } else {
        haystack.chars().collect()
    };
    let need: Vec<char> = if ci {
        needle.to_ascii_lowercase().chars().collect()
    } else {
        needle.chars().collect()
    };
    let mut ni = 0;
    for &c in &hay {
        if ni < need.len() && c == need[ni] {
            ni += 1;
        }
    }
    ni == need.len()
}

/// Weighted ranking blend (docs 04 §4.2).
fn score(c: &Candidate, cfg: &CompletionParams) -> f32 {
    let kind_weight = match c.kind {
        SuggestionKind::History => 3.0,
        _ if c.is_manual => 2.0,
        _ => 1.0,
    };
    let prefix_bonus = if c.is_prefix { 1.0 } else { 0.0 };
    let short_bonus = 1.0 / (1.0 + c.text.len() as f32);
    cfg.w_kind * kind_weight
        + cfg.w_frec * c.frecency
        + cfg.w_prefix * prefix_bonus
        + cfg.w_len * short_bonus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    const RAW: &[(&str, &str, &str, &str)] = &[
        (
            "dir",
            "external",
            "cmd",
            r#"{ "schema": 1, "name": "dir", "options": ["/A", "/B", "/Q", "/S"] }"#,
        ),
        (
            "date",
            "external",
            "cmd",
            r#"{ "schema": 1, "name": "date" }"#,
        ),
        (
            "del",
            "external",
            "cmd",
            r#"{ "schema": 1, "name": "del" }"#,
        ),
        (
            "ls",
            "external",
            "coreutils",
            r#"{ "schema": 1, "name": "ls", "options": ["-a", "-l", "--all", "--color"] }"#,
        ),
        (
            "git",
            "manual",
            "common",
            r#"{ "schema": 1, "name": "git", "options": ["-C", "--version"],
                "subcommands": [
                  { "name": "commit", "options": ["-m", "--amend", "--no-verify"] },
                  { "name": "checkout", "options": ["-b"] },
                  { "name": "remote", "options": ["-v"],
                    "subcommands": [ { "name": "add", "options": ["-t", "-f", "--tags"] } ] }
                ] }"#,
        ),
    ];

    fn engine() -> Engine {
        Engine::with_catalog(Catalog::from_raw(RAW))
    }

    fn ctx<'a>(family: ShellFamily, line: &'a str) -> CompletionContext<'a> {
        CompletionContext {
            family,
            line,
            cursor_col: line.len(),
            now_ms: 1_000_000,
        }
    }

    #[test]
    fn command_context_lists_matching_commands() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "d"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"dir"));
        assert!(texts.contains(&"date"));
        assert!(texts.contains(&"del"));
        assert!(s.iter().all(|x| x.kind == SuggestionKind::Command));
    }

    #[test]
    fn bash_prompt_does_not_show_windows_commands() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "d"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(!texts.contains(&"dir"));
        assert!(!texts.contains(&"date"));
    }

    #[test]
    fn option_context_lists_options() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "dir /"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"/A"));
        assert!(texts.contains(&"/Q"));
        assert!(s.iter().all(|x| x.kind == SuggestionKind::Option));
    }

    #[test]
    fn option_context_narrows_on_prefix() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "dir /Q"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["/Q"]);
    }

    #[test]
    fn unix_long_option_narrows() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "ls --"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"--all"));
        assert!(texts.contains(&"--color"));
        assert!(!texts.contains(&"-a")); // `--` narrows to long options
    }

    #[test]
    fn subcommand_context_lists_children() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git "),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"commit"));
        assert!(texts.contains(&"checkout"));
        assert!(texts.contains(&"remote"));
    }

    #[test]
    fn nested_subcommand_context() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git remote "),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"add"));
    }

    #[test]
    fn subcommand_option_context_uses_active_node() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git commit --"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"--amend"));
        assert!(texts.contains(&"--no-verify"));
        // git's global --version should not outrank; with inheritance it may
        // appear but commit's own options must be present.
    }

    #[test]
    fn nested_option_context() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git remote add -"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"-t"));
        assert!(texts.contains(&"-f"));
    }

    #[test]
    fn ancestor_options_inherited_but_ranked_lower() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git remote add -"),
            &CompletionParams::default(),
        );
        // With inheritance, remote's -v appears, ranked below add's own options.
        let pos_own = s.iter().position(|x| x.text == "-t");
        let pos_anc = s.iter().position(|x| x.text == "-v");
        assert!(pos_own.is_some());
        if let Some(anc) = pos_anc {
            assert!(
                pos_own.unwrap() < anc,
                "active-node option must rank above ancestor"
            );
        }
    }

    #[test]
    fn unknown_subcommand_falls_back_to_command_options() {
        let e = engine();
        let h = CompletionHistory::new(10);
        // `git frobnicate -` → walk stops at git; offers git's options.
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git frobnicate -"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(texts.contains(&"-C"));
    }

    #[test]
    fn history_beats_catalog_and_dedups_with_h_tag() {
        let e = engine();
        let mut h = CompletionHistory::new(10);
        // Use `dir` a lot this session.
        for t in [1000u64, 2000, 3000] {
            h.record(ShellFamily::Cmd, "dir", t);
        }
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "d"),
            &CompletionParams::default(),
        );
        // `dir` appears once, tagged History, and ranks first.
        let dir_entries: Vec<_> = s.iter().filter(|x| x.text == "dir").collect();
        assert_eq!(dir_entries.len(), 1, "dir must be deduped");
        assert_eq!(dir_entries[0].kind, SuggestionKind::History);
        assert_eq!(s[0].text, "dir", "frecent history ranks first");
    }

    #[test]
    fn prefix_beats_fuzzy() {
        let e = engine();
        let h = CompletionHistory::new(10);
        // token "dt" fuzzy-matches "date"; "d" prefix beats it — use "da".
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "da"),
            &CompletionParams::default(),
        );
        assert_eq!(s[0].text, "date");
    }

    #[test]
    fn case_sensitivity_per_family() {
        let e = engine();
        let h = CompletionHistory::new(10);
        // Cmd is case-insensitive: "DI" → dir.
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "DI"),
            &CompletionParams::default(),
        );
        assert!(s.iter().any(|x| x.text == "dir"));
        // Unix is case-sensitive: "LS" must not match "ls".
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "LS"),
            &CompletionParams::default(),
        );
        assert!(!s.iter().any(|x| x.text == "ls"));
    }

    #[test]
    fn remainder_is_append_only() {
        let s = Suggestion {
            text: "dir".into(),
            kind: SuggestionKind::Command,
            description: None,
            match_start: 0,
            match_len: 2,
            score: 0.0,
            replace_from: 0,
        };
        assert_eq!(s.remainder("di"), "r");
        assert_eq!(s.remainder("dir"), "");
        // Non-prefix → empty (fuzzy accept gated off).
        assert_eq!(s.remainder("xyz"), "");
    }

    #[test]
    fn min_prefix_len_suppresses_short_tokens() {
        let e = engine();
        let h = CompletionHistory::new(10);
        let mut cfg = CompletionParams::default();
        cfg.min_prefix_len = 2;
        let s = e.suggest(&h, &ctx(ShellFamily::Cmd, "d"), &cfg);
        assert!(s.is_empty());
    }

    #[test]
    fn recorded_secret_never_suggested() {
        let e = engine();
        let mut h = CompletionHistory::new(10);
        // Even if a raw secret is injected into the ring (bypassing capture),
        // the suggestion-time guard drops it.
        h.record(
            ShellFamily::Unix,
            "deploy --token ghp_0123456789abcdefghij",
            1000,
        );
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "deploy "),
            &CompletionParams::default(),
        );
        assert!(s.iter().all(|x| !x.text.contains("ghp_")));
    }

    #[test]
    fn sources_toggle_off_history() {
        let e = engine();
        let mut h = CompletionHistory::new(10);
        h.record(ShellFamily::Cmd, "docker ps", 1000);
        let mut cfg = CompletionParams::default();
        cfg.sources.memory = false;
        let s = e.suggest(&h, &ctx(ShellFamily::Cmd, "d"), &cfg);
        assert!(!s.iter().any(|x| x.text == "docker"));
    }

    #[test]
    fn embedded_catalog_suggests_cmd_commands_for_d() {
        // Uses the REAL compile-time embedded catalogs (not the inline fixture),
        // mirroring what the app does via `Engine::from_embedded()`.
        let e = Engine::from_embedded();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Cmd, "d"),
            &CompletionParams::default(),
        );
        let texts: Vec<_> = s.iter().map(|x| x.text.as_str()).collect();
        assert!(
            !s.is_empty(),
            "embedded catalog returned no suggestions for 'd'"
        );
        assert!(
            texts.contains(&"dir") || texts.contains(&"date") || texts.contains(&"del"),
            "expected dir/date/del among embedded cmd suggestions, got {texts:?}"
        );
    }

    #[test]
    fn embedded_catalog_suggests_unix_commands_for_l() {
        let e = Engine::from_embedded();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "l"),
            &CompletionParams::default(),
        );
        assert!(
            !s.is_empty(),
            "embedded catalog returned no unix suggestions for 'l'"
        );
        assert!(
            s.iter().any(|x| x.text == "ls"),
            "expected ls among unix suggestions"
        );
    }

    #[test]
    fn option_description_flows_from_manual_catalog() {
        // `git checkout -b` should surface the `-b` option carrying its
        // `new-branch` argument hint (docs: object flag form with description).
        let e = Engine::from_embedded();
        let h = CompletionHistory::new(10);
        let s = e.suggest(
            &h,
            &ctx(ShellFamily::Unix, "git checkout -b"),
            &CompletionParams::default(),
        );
        let b = s
            .iter()
            .find(|x| x.text == "-b")
            .expect("expected -b option for `git checkout`");
        assert_eq!(b.description.as_deref(), Some("new-branch"));
    }
}
