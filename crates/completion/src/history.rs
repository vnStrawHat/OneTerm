//! In-session command history (`memory` source).
//!
//! A bounded ring buffer **per shell family** (docs/auto-completion/02 §2.3), each
//! entry carrying a small frecency record so ranking can favor recent/frequent
//! commands (docs 04 §4.2). Lines are recorded **already redacted**
//! (docs 08 §2). This store is process-global and non-persistent — it lives in
//! `oneterm-state` as an `Entity<CompletionHistory>` (docs 01 §4) — but the data
//! + logic here are gpui-free and unit-testable.

use std::collections::HashMap;

use crate::family::ShellFamily;

/// A recorded command line plus its frecency bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrecencyRecord {
    pub line: String,
    pub use_count: u32,
    /// Last-used timestamp in milliseconds (supplied by the caller — the engine
    /// stays clock-free, docs 04 §1).
    pub last_used_ms: u64,
}

impl FrecencyRecord {
    /// `use_count * recency_decay(now - last_used)`. Recent + frequent entries
    /// float to the top (docs 04 §4.2).
    pub fn frecency(&self, now_ms: u64) -> f32 {
        let age_hours = now_ms.saturating_sub(self.last_used_ms) as f64 / 3_600_000.0;
        let decay = 1.0 / (1.0 + age_hours);
        (self.use_count as f64 * decay) as f32
    }
}

/// A single history match handed to the engine.
#[derive(Debug, Clone)]
pub struct HistoryHit {
    /// The suggested text (a whole command line or a first token).
    pub text: String,
    /// Frecency score of the source entry.
    pub frecency: f32,
    /// Length of the matched prefix (for highlight).
    pub match_len: usize,
}

/// A bounded, deduped ring buffer of command lines for one shell family.
#[derive(Debug, Clone)]
pub struct CommandRing {
    entries: Vec<FrecencyRecord>,
    capacity: usize,
}

impl CommandRing {
    fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
        }
    }

    /// Record a line: update frecency if it already exists, else insert (evicting
    /// the oldest entry when at capacity).
    fn record(&mut self, line: &str, now_ms: u64) {
        if self.capacity == 0 {
            return;
        }
        if let Some(rec) = self.entries.iter_mut().find(|r| r.line == line) {
            rec.use_count = rec.use_count.saturating_add(1);
            rec.last_used_ms = now_ms;
            return;
        }
        self.entries.push(FrecencyRecord {
            line: line.to_string(),
            use_count: 1,
            last_used_ms: now_ms,
        });
        self.evict();
    }

    fn evict(&mut self) {
        while self.entries.len() > self.capacity {
            // Evict the oldest (smallest last_used_ms).
            if let Some((idx, _)) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, r)| r.last_used_ms)
            {
                self.entries.remove(idx);
            } else {
                break;
            }
        }
    }

    fn set_capacity(&mut self, n: usize) {
        self.capacity = n;
        self.evict();
    }
}

/// The global, cross-tab history store: one ring per shell family.
#[derive(Debug, Clone)]
pub struct CompletionHistory {
    per_family: HashMap<ShellFamily, CommandRing>,
    capacity: usize,
}

impl Default for CompletionHistory {
    fn default() -> Self {
        Self::new(500)
    }
}

impl CompletionHistory {
    /// Create a history store with the given per-family ring capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            per_family: HashMap::new(),
            capacity,
        }
    }

    /// Record an **already-redacted** command line under `family`.
    pub fn record(&mut self, family: ShellFamily, redacted_line: &str, now_ms: u64) {
        let line = redacted_line.trim();
        if line.is_empty() || self.capacity == 0 {
            return;
        }
        self.per_family
            .entry(family)
            .or_insert_with(|| CommandRing::new(self.capacity))
            .record(line, now_ms);
    }

    /// The raw records for a family (most useful for the engine's context-aware
    /// matching). Empty if none recorded.
    pub fn entries(&self, family: ShellFamily) -> &[FrecencyRecord] {
        self.per_family
            .get(&family)
            .map(|r| r.entries.as_slice())
            .unwrap_or(&[])
    }

    /// Whole-line-prefix matches for `token` (case per family). Used for command
    /// recall in subcommand/argument context.
    pub fn matches(&self, family: ShellFamily, token: &str, now_ms: u64) -> Vec<HistoryHit> {
        let ci = family.case_insensitive();
        self.entries(family)
            .iter()
            .filter(|r| prefix_match(&r.line, token, ci))
            .map(|r| HistoryHit {
                text: r.line.clone(),
                frecency: r.frecency(now_ms),
                match_len: token.len(),
            })
            .collect()
    }

    /// Set the per-family ring capacity. `0` disables/clears the store.
    pub fn set_capacity(&mut self, n: usize) {
        self.capacity = n;
        if n == 0 {
            self.clear();
            return;
        }
        for ring in self.per_family.values_mut() {
            ring.set_capacity(n);
        }
    }

    /// The current per-family capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear all recorded history (the "Clear history" action).
    pub fn clear(&mut self) {
        self.per_family.clear();
    }

    /// Whether the store holds no entries for any family.
    pub fn is_empty(&self) -> bool {
        self.per_family.values().all(|r| r.entries.is_empty())
    }
}

/// Family-aware prefix match.
pub(crate) fn prefix_match(haystack: &str, prefix: &str, case_insensitive: bool) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if haystack.len() < prefix.len() {
        return false;
    }
    let head = &haystack[..prefix.len()];
    if case_insensitive {
        head.eq_ignore_ascii_case(prefix)
    } else {
        head == prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_match_whole_line() {
        let mut h = CompletionHistory::new(10);
        h.record(ShellFamily::Unix, "git commit -m msg", 1000);
        let hits = h.matches(ShellFamily::Unix, "git c", 2000);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "git commit -m msg");
    }

    #[test]
    fn family_partitioned() {
        let mut h = CompletionHistory::new(10);
        h.record(ShellFamily::Cmd, "dir /Q", 1000);
        assert!(h.matches(ShellFamily::Unix, "dir", 2000).is_empty());
        assert_eq!(h.matches(ShellFamily::Cmd, "dir", 2000).len(), 1);
    }

    #[test]
    fn repeat_updates_frecency_not_duplicates() {
        let mut h = CompletionHistory::new(10);
        h.record(ShellFamily::Unix, "docker ps", 1000);
        h.record(ShellFamily::Unix, "docker ps", 2000);
        assert_eq!(h.entries(ShellFamily::Unix).len(), 1);
        assert_eq!(h.entries(ShellFamily::Unix)[0].use_count, 2);
    }

    #[test]
    fn ring_evicts_oldest() {
        let mut h = CompletionHistory::new(2);
        h.record(ShellFamily::Unix, "a", 1000);
        h.record(ShellFamily::Unix, "b", 2000);
        h.record(ShellFamily::Unix, "c", 3000);
        let lines: Vec<_> = h
            .entries(ShellFamily::Unix)
            .iter()
            .map(|r| r.line.clone())
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(!lines.contains(&"a".to_string())); // oldest evicted
        assert!(lines.contains(&"c".to_string()));
    }

    #[test]
    fn set_capacity_zero_clears() {
        let mut h = CompletionHistory::new(10);
        h.record(ShellFamily::Unix, "ls", 1000);
        h.set_capacity(0);
        assert!(h.is_empty());
        // Recording is disabled while capacity is 0.
        h.record(ShellFamily::Unix, "ls", 2000);
        assert!(h.is_empty());
    }

    #[test]
    fn frecency_recent_beats_old() {
        let recent = FrecencyRecord {
            line: "x".into(),
            use_count: 1,
            last_used_ms: 3_600_000,
        };
        let old = FrecencyRecord {
            line: "y".into(),
            use_count: 1,
            last_used_ms: 0,
        };
        let now = 3_600_000 + 3_600_000; // 1h after recent
        assert!(recent.frecency(now) > old.frecency(now));
    }
}
