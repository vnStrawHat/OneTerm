//! OSC 9;7 `seq` dedup helper (spec §4.1 / §8.3).
//!
//! `seq` dedup is per-(terminal, agent) state: each agent emitting into a
//! terminal has its own monotonic `seq` counter, and the receiver must drop
//! any event whose `seq` is `<=` the last applied `seq` for that agent. This
//! guards against re-emissions, replays, and out-of-order delivery.
//!
//! The dedup state (`last_agent_seq: HashMap<agent_id, last_seq>`) is owned by
//! the listener's `SessionState` (it is terminal-scoped, not parser-scoped),
//! but the **decision logic** is shared here so both backends (`local-shell`,
//! `ssh`) apply identical rules. Tested once in `osc_agent/tests.rs`.

use std::collections::HashMap;

use super::AgentStatusEvent;

/// Decide whether an OSC 9;7 event should be applied, updating the per-agent
/// `seq` watermark in place.
///
/// Returns `true` if `ev.seq()` is strictly greater than the last applied
/// `seq` for `ev.agent()` (and updates the watermark); `false` otherwise
/// (stale/equal/duplicate — drop silently, spec §3.3).
///
/// Callers hold the `SessionState` lock when calling this (the watermark map
/// lives inside `SessionState::last_agent_seq`).
pub fn should_apply(last_seq: &mut HashMap<String, u64>, ev: &AgentStatusEvent) -> bool {
    let agent = ev.agent();
    let seq = ev.seq();
    let last = last_seq.entry(agent.to_string()).or_insert(0);
    if seq <= *last {
        false
    } else {
        *last = seq;
        true
    }
}
