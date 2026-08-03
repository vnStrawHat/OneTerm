//! Per-context candidate gathering + finalization for the suggestion engine
//! (docs 04 §3–§4).
use super::scoring::{match_option, match_token, precedence, score, text_eq};
use super::{Candidate, Engine, Query, Suggestion, SuggestionKind};
use crate::catalog::names_eq;
use crate::family::ShellFamily;
use crate::history::prefix_match;
use crate::params::CompletionParams;
use crate::parse::ParsedLine;
use crate::redact;
impl Engine {
    pub(super) fn gather_commands(
        &self,
        q: &Query,
        p: &ParsedLine,
        token: &str,
        out: &mut Vec<Candidate>,
    ) {
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
                        match_len: mlen,
                        frecency: rec.frecency(q.ctx.now_ms),
                        replace_from: p.token_start,
                    });
                }
            }
        }
    }
    pub(super) fn gather_options(
        &self,
        q: &Query,
        p: &ParsedLine,
        token: &str,
        out: &mut Vec<Candidate>,
    ) {
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
    pub(super) fn gather_history_whole_line(&self, q: &Query, out: &mut Vec<Candidate>) {
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
                    match_len: line_prefix.len(),
                    frecency: rec.frecency(ctx.now_ms),
                    replace_from: 0,
                });
            }
        }
    }
    /// Score, dedup, drop secrets, sort, and truncate.
    pub(super) fn finalize(
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
                match_len: c.match_len,
                score,
                replace_from: c.replace_from,
            })
            .collect()
    }
}
