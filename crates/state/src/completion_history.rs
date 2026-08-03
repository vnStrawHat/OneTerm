//! `GlobalCompletionHistory` — the process-global, cross-tab command history.
//!
//! The `memory` completion source must be shared across every Terminal Tab but
//! reset when OneTerm exits — exactly the lifetime of a process-global GPUI
//! entity (docs/auto-completion/01 §4). This mirrors the [`crate::AgentRegistry`]
//! global pattern: a thin `Entity<CompletionHistory>` wrapper registered once at
//! startup. Nothing here is persisted — the store lives only in RAM.

use gpui::{App, AppContext, Entity, Global};

pub use oneterm_completion::CompletionHistory;

/// Global wrapper for `Entity<CompletionHistory>`.
pub struct GlobalCompletionHistory(pub Entity<CompletionHistory>);

impl Global for GlobalCompletionHistory {}

impl GlobalCompletionHistory {
    /// The global `Entity<CompletionHistory>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<CompletionHistory> {
        cx.global::<GlobalCompletionHistory>().0.clone()
    }

    /// The global `Entity<CompletionHistory>` if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<CompletionHistory>> {
        cx.try_global::<GlobalCompletionHistory>()
            .map(|g| g.0.clone())
    }

    /// Initialize the global history store once (called from `app::init`).
    pub fn init(cx: &mut App) {
        if cx.try_global::<GlobalCompletionHistory>().is_none() {
            let entity = cx.new(|_| CompletionHistory::default());
            cx.set_global(GlobalCompletionHistory(entity));
        }
    }
}
