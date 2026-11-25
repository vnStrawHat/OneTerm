//! Compiled rule set — keywords (Aho-Corasick) + structural regexes + probes.
//!
//! Built once at startup (via [`RuleSet::global`]) and shared across all
//! views. No per-line interpretation of JSON grammars — the vocabulary is
//! `&'static [&str]` compiled into one SIMD-accelerated Aho-Corasick. See §5
//! and §Q2 of the design doc.

use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

use crate::class::Class;

// ── Keyword vocabulary (Rust statics for v1 — see §Q2) ─────────────────────

/// Error keywords → [`Class::Error`].
const ERROR_WORDS: &[&str] = &[
    "bad",
    "cannot",
    "denied",
    "deprecated",
    "disabled",
    "error",
    "errors",
    "fail",
    "failed",
    "failure",
    "false",
    "important",
    "incorrect",
    "invalid",
    "none",
    "refused",
    "unknown",
    "unsupported",
    "not supported",
    "wrong",
];

/// Success keywords → [`Class::Success`].
const SUCCESS_WORDS: &[&str] = &[
    "can",
    "correct",
    "correctly",
    "known",
    "ok",
    "pass",
    "passed",
    "success",
    "successful",
    "successfully",
    "supported",
    "true",
    "valid",
    "yes",
];

/// Warning keywords → [`Class::Warn`].
const WARN_WORDS: &[&str] = &[
    "closed",
    "disconnected",
    "exited",
    "skipped",
    "stopped",
    "sudo",
    "terminated",
    "warning",
    "warn",
];

/// Info keywords → [`Class::Info`].
const INFO_WORDS: &[&str] = &[
    "access",
    "authentication",
    "connection",
    "disconnection",
    "info",
    "login",
    "operation",
    "password",
    "permission",
];

/// Debug keywords → [`Class::Debug`].
const DEBUG_WORDS: &[&str] = &["debug", "trace", "verbose"];

/// All keyword patterns in priority order. Order matters for ties (same
/// length, different class): earlier patterns win. We put more specific
/// classes first to avoid e.g. "warning" resolving to `Error` instead of
/// `Warn` (it was removed from `ERROR_WORDS`, but the ordering principle
/// holds for any future overlaps).
const ALL_PATTERNS: &[&[&str]] = &[
    DEBUG_WORDS,
    WARN_WORDS,
    INFO_WORDS,
    SUCCESS_WORDS,
    ERROR_WORDS,
];

const ALL_CLASSES: &[Class] = &[
    Class::Debug,
    Class::Warn,
    Class::Info,
    Class::Success,
    Class::Error,
];

/// Compiled rule set — one Aho-Corasick over all keywords + structural regexes.
pub struct RuleSet {
    /// One Aho-Corasick automaton over all keyword patterns.
    pub(crate) keywords: AhoCorasick,
    /// Parallel to the Aho-Corasick patterns → which `Class` each pattern maps to.
    pub(crate) keyword_class: Vec<Class>,
    /// IPv6 address regex.
    pub(crate) ipv6: regex::Regex,
    /// MAC address regex.
    pub(crate) mac: regex::Regex,
    /// Date/time regex.
    pub(crate) datetime: regex::Regex,
}

impl RuleSet {
    /// Build a `RuleSet` from the static keyword tables + compiled regexes.
    fn build() -> Self {
        // Flatten all patterns, keeping a parallel class map.
        let mut patterns: Vec<&str> = Vec::new();
        let mut keyword_class: Vec<Class> = Vec::new();
        for (words, class) in ALL_PATTERNS.iter().zip(ALL_CLASSES.iter()) {
            for &w in *words {
                patterns.push(w);
                keyword_class.push(*class);
            }
        }

        let keywords = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .ascii_case_insensitive(true)
            .build(&patterns)
            .expect("Aho-Corasick build failed (static patterns)");

        Self {
            keywords,
            keyword_class,
            ipv6: regex::Regex::new(concat!(
                // Compressed forms MUST come before full form (regex uses leftmost-first alternation).
                r"(?:(?:[0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*",  // prefix::suffix
                r"|::[0-9a-fA-F]{1,4}(?::[0-9a-fA-F]{1,4})*",  // leading ::suffix
                r"|(?:[0-9a-fA-F]{1,4}:){1,6}:",  // trailing prefix::
                r"|(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}",  // full form: exactly 8 groups
                r")",
            ))
            .expect("IPv6 regex is valid"),
            mac: regex::Regex::new(r"\b[0-9a-fA-F]{2}(?::[0-9a-fA-F]{2}){5}\b")
                .expect("MAC regex is valid"),
            datetime: regex::Regex::new(concat!(
                r"\b(?:",
                // ISO / numeric date + optional time / timezone / AM-PM.
                r"\d{4}[-/]\d{2}[-/]\d{2}(?:[ T]\d{1,2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:\s*(?:Z|UTC|GMT|AM|PM|[+-]\d{2}:?\d{2}))?)?",
                r"|\d{2}/\d{2}/\d{4}(?:[ T]\d{1,2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:\s*(?:Z|UTC|GMT|AM|PM|[+-]\d{2}:?\d{2}))?)?",
                // Time with optional AM/PM / timezone.
                r"|\d{1,2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:\s*(?:AM|PM|UTC|GMT|Z|[+-]\d{2}:?\d{2}))?",
                // Syslog style: Wed Oct 25 10:15:30 UTC 2023
                // Also covers: Mon Jul 13 06:38:56 AM UTC 2026 (with optional AM/PM).
                r"|(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)[a-z]*\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}\s+\d{1,2}:\d{2}(?::\d{2})?(?:\s*(?:AM|PM))?(?:\s+(?:UTC|GMT|[+-]\d{2}:?\d{2}))?\s+\d{4}",
                // Standalone month / weekday names.
                r"|(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*",
                r"|(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun)[a-z]*",
                r")\b",
            ))
            .expect("DateTime regex is valid"),
        }
    }

    /// Get the global shared `RuleSet` (built once via `LazyLock`).
    pub fn global() -> &'static RuleSet {
        &GLOBAL_RULES
    }

    /// Map a keyword pattern index → `Class`.
    pub(crate) fn class_for_pattern(&self, pattern_idx: usize) -> Class {
        self.keyword_class
            .get(pattern_idx)
            .copied()
            .unwrap_or(Class::Default)
    }
}

static GLOBAL_RULES: LazyLock<RuleSet> = LazyLock::new(RuleSet::build);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_without_panic() {
        let _ = RuleSet::global();
    }

    #[test]
    fn keyword_class_mapping() {
        let rs = RuleSet::global();
        // Every pattern has a class.
        assert_eq!(rs.keyword_class.len(), rs.keywords.patterns_len());
    }

    #[test]
    fn ipv6_matches() {
        let rs = RuleSet::global();
        assert!(rs.ipv6.is_match("2001:db8::1"));
        assert!(rs.ipv6.is_match("fe80::1"));
        // Full compressed address with multiple groups after ::
        assert!(rs.ipv6.is_match("fe80::1c2d:3e4f:5a6b"));
        assert!(rs.ipv6.is_match("2607:f8b0:4004:80a::200e"));
        // Zone ID
        assert!(rs.ipv6.is_match("fe80::1c2d:3e4f:5a6b%eth0"));
        // Leading ::
        assert!(rs.ipv6.is_match("::1"));
        assert!(rs.ipv6.is_match("::1c2d:3e4f:5a6b"));
        // Full form
        assert!(rs.ipv6.is_match("2001:db8:0:0:0:0:0:1"));
    }

    #[test]
    fn ipv6_full_match_length() {
        let rs = RuleSet::global();
        // Verify the regex matches the ENTIRE compressed address, not just prefix.
        let m = rs.ipv6.find("fe80::1c2d:3e4f:5a6b").unwrap();
        assert_eq!(m.start(), 0);
        assert_eq!(m.end(), "fe80::1c2d:3e4f:5a6b".len());

        let m = rs.ipv6.find("2607:f8b0:4004:80a::200e").unwrap();
        assert_eq!(m.start(), 0);
        assert_eq!(m.end(), "2607:f8b0:4004:80a::200e".len());

        // Zone ID NOT included — match stops at the address.
        let m = rs.ipv6.find("fe80::1c2d:3e4f:5a6b%eth0").unwrap();
        assert_eq!(m.end(), "fe80::1c2d:3e4f:5a6b".len());
    }

    #[test]
    fn ipv6_does_not_match_clock_time() {
        // Three colon-separated groups look like a short IPv6; must not match.
        let rs = RuleSet::global();
        assert!(!rs.ipv6.is_match("09:30:45"));
        assert!(!rs.ipv6.is_match("11:59:00"));
    }

    #[test]
    fn mac_matches() {
        let rs = RuleSet::global();
        assert!(rs.mac.is_match("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn datetime_matches() {
        let rs = RuleSet::global();
        // Standalone pieces.
        assert!(rs.datetime.is_match("2026-06-23"));
        assert!(rs.datetime.is_match("14:30"));
        assert!(rs.datetime.is_match("Jan"));

        // Full datetimes from the reference examples.
        let full = rs.datetime.find("2024-01-15 09:30:45").unwrap();
        assert_eq!(full.as_str(), "2024-01-15 09:30:45");

        let slash = rs.datetime.find("12/25/2023 11:59 PM").unwrap();
        assert_eq!(slash.as_str(), "12/25/2023 11:59 PM");

        let syslog = rs.datetime.find("Wed Oct 25 10:15:30 UTC 2023").unwrap();
        assert_eq!(syslog.as_str(), "Wed Oct 25 10:15:30 UTC 2023");

        let syslog_am = rs.datetime.find("Mon Jul 13 06:38:56 AM UTC 2026").unwrap();
        assert_eq!(syslog_am.as_str(), "Mon Jul 13 06:38:56 AM UTC 2026");

        let iso = rs.datetime.find("2024-03-01T14:22:08.123Z").unwrap();
        assert_eq!(iso.as_str(), "2024-03-01T14:22:08.123Z");
    }
}
