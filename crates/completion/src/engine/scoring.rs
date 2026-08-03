//! Matching + ranking helpers for the suggestion engine (docs 04 §4).

use super::{Candidate, SuggestionKind};
use crate::history::prefix_match;
use crate::params::CompletionParams;

/// Precedence for dedup tag-keeping: History > manual C/O > external C/O.
pub(super) fn precedence(c: &Candidate) -> u8 {
    match c.kind {
        SuggestionKind::History => 3,
        _ if c.is_manual => 2,
        _ => 1,
    }
}

/// Family-aware text equality for dedup.
pub(super) fn text_eq(a: &str, b: &str, ci: bool) -> bool {
    if ci {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// Match a command/subcommand name against a token: prefix first, then fuzzy.
/// Returns `(is_prefix, match_len)` or `None`.
pub(super) fn match_token(
    name: &str,
    token: &str,
    ci: bool,
    cfg: &CompletionParams,
) -> Option<(bool, usize)> {
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
pub(super) fn match_option(
    flag: &str,
    token: &str,
    cfg: &CompletionParams,
) -> Option<(bool, usize)> {
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
pub(super) fn score(c: &Candidate, cfg: &CompletionParams) -> f32 {
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
