//! OSC 9;7 `seq` dedup helper (spec §4.1 / §8.3).
//!
//! `seq` dedup is per-(terminal, agent) state: each agent emitting into a
//! terminal has its own monotonic `seq` counter, and the receiver must drop
//! any event whose `seq` is `<=` the last applied `seq` for that agent. This
//! guards against re-emissions, replays, and out-of-order delivery.
//!
//! The dedup state ([`AgentSeqWatermarks`]) is owned by the router's
//! `SessionState` (it is terminal-scoped, not parser-scoped), but the
//! **decision logic** is shared here so both backends (`local-shell`, `ssh`)
//! apply identical rules. Tested in `osc_agent/receiver_tests.rs`.
//!
//! The `agent` id is terminal-controlled, so the watermark table is bounded
//! (SEC-04): at most [`MAX_TRACKED_AGENTS`] ids are kept and the least
//! recently seen one is evicted when a new id arrives at capacity. Ids are
//! also capped at parse time ([`super::MAX_AGENT_ID_BYTES`]).

use std::collections::HashMap;

use super::AgentStatusEvent;

/// Maximum number of distinct agent ids whose `seq` watermark is tracked per
/// terminal. Real terminals see a handful; a hostile program cycling ids only
/// churns this table instead of growing memory without bound.
pub const MAX_TRACKED_AGENTS: usize = 64;

/// Bounded per-agent `seq` watermark table.
#[derive(Debug, Default, Clone)]
pub struct AgentSeqWatermarks {
    /// agent id → (last applied `seq`, last-use tick for eviction).
    entries: HashMap<String, (u64, u64)>,
    /// Monotonic counter stamped on every access.
    tick: u64,
}

impl AgentSeqWatermarks {
    /// The last applied `seq` for `agent`, if it is currently tracked.
    pub fn get(&self, agent: &str) -> Option<u64> {
        self.entries.get(agent).map(|(seq, _)| *seq)
    }

    /// Number of agent ids currently tracked.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no agent id is tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn evict_least_recent(&mut self) {
        let victim = self
            .entries
            .iter()
            .min_by_key(|(_, (_, tick))| *tick)
            .map(|(agent, _)| agent.clone());
        if let Some(agent) = victim {
            self.entries.remove(&agent);
        }
    }
}

/// Decide whether an OSC 9;7 event should be applied, updating the per-agent
/// `seq` watermark in place.
///
/// Returns `true` if `ev.seq()` is strictly greater than the last applied
/// `seq` for `ev.agent()` (and updates the watermark); `false` otherwise
/// (stale/equal/duplicate — drop silently, spec §3.3). An agent id seen for
/// the first time starts at watermark 0, so its first event needs `seq >= 1`.
///
/// Callers hold the `SessionState` lock when calling this (the watermark table
/// lives inside `SessionState::last_agent_seq`).
pub fn should_apply(watermarks: &mut AgentSeqWatermarks, ev: &AgentStatusEvent) -> bool {
    let agent = ev.agent();
    let seq = ev.seq();
    let tick = watermarks.next_tick();
    if let Some((last, last_tick)) = watermarks.entries.get_mut(agent) {
        *last_tick = tick;
        if seq <= *last {
            return false;
        }
        *last = seq;
        return true;
    }
    // Unknown agent: watermark 0.
    if seq == 0 {
        return false;
    }
    if watermarks.entries.len() >= MAX_TRACKED_AGENTS {
        watermarks.evict_least_recent();
    }
    watermarks.entries.insert(agent.to_string(), (seq, tick));
    true
}
