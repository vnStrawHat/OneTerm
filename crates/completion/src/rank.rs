//! Matching and ranking (specs/completion, "A suggestion").

use crate::catalog::Source;
use crate::engine::{Kind, Suggestion};
use crate::{Config, Family, redact};

/// A suggestion before it is ranked.
pub(crate) struct Candidate {
    pub text: String,
    pub kind: Kind,
    pub source: Source,
    pub detail: Option<String>,
    pub prefix: bool,
    pub matched: usize,
    /// 0 for a catalog's, below for an ancestor's option, the record's for the history's.
    pub frecency: f32,
    /// Where the bytes it replaces start; they end at the cursor.
    pub start: usize,
}

impl Candidate {
    pub(crate) fn new(text: &str, kind: Kind, source: Source, start: usize) -> Candidate {
        Candidate {
            text: text.to_string(),
            kind,
            source,
            detail: None,
            prefix: true,
            matched: 0,
            frecency: 0.0,
            start,
        }
    }

    fn score(&self) -> f32 {
        let source = match self.source {
            Source::History => 3.0,
            Source::Manual => 2.0,
            Source::External => 1.0,
        };
        10.0 * source + 6.0 * self.frecency + 1.0 / (1.0 + self.text.len() as f32)
    }
}

/// How `text` matches what is typed: `(prefix, matched bytes)`, by prefix, else (with `fuzzy`)
/// as a subsequence.
pub(crate) fn matches(text: &str, typed: &str, family: Family, fuzzy: bool) -> Option<(bool, usize)> {
    if family.starts(text, typed) {
        return Some((true, typed.len()));
    }
    let fold = |c: char| if family.ignores_case() { c.to_ascii_lowercase() } else { c };
    let mut rest = text.chars().map(fold);
    let subsequence = typed.chars().map(fold).all(|wanted| rest.any(|c| c == wanted));
    (fuzzy && subsequence).then_some((false, 0))
}

/// Drops secrets, ranks, keeps each text once, and bounds the list.
pub(crate) fn finish(
    candidates: Vec<Candidate>,
    line: &str,
    cursor: usize,
    family: Family,
    config: &Config,
) -> Vec<Suggestion> {
    let mut ranked: Vec<(f32, Candidate)> = candidates
        .into_iter()
        .filter(|candidate| {
            !(config.redact_sensitive
                && candidate.source == Source::History
                && redact::contains_secret(&candidate.text))
        })
        .map(|candidate| (candidate.score(), candidate))
        .collect();
    // Every prefix match before every fuzzy one; the sort is stable, so equal ones keep their order.
    ranked.sort_by(|(a, x), (b, y)| y.prefix.cmp(&x.prefix).then(b.total_cmp(a)));
    let mut kept: Vec<Candidate> = Vec::new();
    for (_, candidate) in ranked {
        match kept.iter_mut().find(|other| family.same(&other.text, &candidate.text)) {
            Some(other) => {
                other.source = other.source.max(candidate.source);
                other.detail = other.detail.take().or(candidate.detail);
            }
            None => kept.push(candidate),
        }
    }
    kept.truncate(config.max_visible_items.max(1).saturating_mul(4));
    // A list of one that is already what is typed adds nothing.
    if let [only] = kept.as_slice()
        && line.get(only.start..cursor) == Some(only.text.as_str())
    {
        return Vec::new();
    }
    kept.into_iter()
        .map(|candidate| Suggestion {
            range: candidate.start..cursor,
            text: candidate.text,
            kind: candidate.kind,
            source: candidate.source,
            detail: candidate.detail,
            matched: candidate.matched,
            prefix: candidate.prefix,
        })
        .collect()
}
