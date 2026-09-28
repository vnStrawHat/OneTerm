//! OneTerm cross-feature runtime state + injection.
//!
//! This crate sits *below* both the shell and the feature crates, which is what
//! keeps the dependency graph acyclic. It owns:
//!
//! - **runtime state** shared across features: [`AppState`] (active SFTP
//!   backend / cwd source per workspace), [`AgentRegistry`] (the folded agent-status
//!   agent model behind the Agent Panel), [`CompletionHistory`],
//!   [`InputChannelRegistry`] (broadcast input channel membership + fan-out);
//! - **injection**: [`AppServices`] — the single composition-root bundle through
//!   which features receive the session factory and contribute the workspace
//!   commands / active-terminal metrics / agent focuser the shell and other
//!   features call without depending on them;
//! - **shared shell contracts**: `docks.json` document ownership
//!   ([`dock_persistence`]), panel names and dock traversal helpers.

pub mod active_terminal;
pub mod agent_focus;
mod agent_model;
pub mod agent_registry;
pub mod app_state;
pub mod commands;
pub mod completion_history;
pub mod dock_persistence;
pub mod dock_util;
pub mod form_dialog;
pub mod input_channel_registry;
pub mod panel_names;
pub mod persist_queue;
pub mod services;

pub use agent_registry::{
    AgentCard, AgentNav, AgentRegistry, AgentStateCounts, ApprovalInfo, FileEntry, Grouping,
    Lifecycle, ModelInfo, ToolRun,
};
pub use app_state::AppState;
pub use completion_history::{CompletionHistory, GlobalCompletionHistory};
pub use input_channel_registry::{BroadcastInput, InputChannelRegistry};
pub use persist_queue::PersistQueue;
pub use services::AppServices;

/// How long a repeating UI timer should sleep to wake on the next multiple of
/// `interval` since the Unix epoch.
///
/// Every timer that repaints the window (the cursor blink, the status-bar
/// indicators) sleeps with this, so timers whose intervals divide each other
/// wake together and gpui draws one frame for all of them instead of one each
/// (`US-0145`). A plain `timer(interval)` loop starts wherever its entity was
/// created and drifts by its own work every lap, so the blink and the clock
/// used to land in different frames.
pub fn until_next_tick(interval: std::time::Duration) -> std::time::Duration {
    let interval_ns = interval.as_nanos().max(1);
    let now_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut wait_ns = interval_ns - now_ns % interval_ns;
    // A timer that fired a hair before the tick must not tick again right away.
    if wait_ns < interval_ns / 4 {
        wait_ns += interval_ns;
    }
    std::time::Duration::from_nanos(u64::try_from(wait_ns).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tick_tests {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn a_tick_lands_on_the_shared_grid() {
        for interval in [Duration::from_millis(500), Duration::from_secs(2)] {
            let wait = super::until_next_tick(interval);
            assert!(
                wait >= interval / 4 && wait <= interval + interval / 4,
                "{wait:?}"
            );
            let wake = SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + wait;
            // A multiple of the interval, give or take the time between the
            // two clock reads.
            let off = wake.as_millis() % interval.as_millis();
            assert!(
                off < 50 || interval.as_millis() - off < 50,
                "{off} ms off the grid"
            );
        }
    }
}
