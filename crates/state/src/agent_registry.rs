//! `AgentRegistry` — the aggregated, folded display model behind the Agent Panel.
//!
//! OSC 9;7 events arrive as a **stream** of small, single-`type` messages
//! (`state`, `session`, `heartbeat`, `model`, `tool_call`, `file`, `approval` —
//! see `docs/osc-agent-status.md` §4.2). A useful panel needs the *folded*
//! history of that stream per agent: the current lifecycle state **and** the
//! active model + context usage, the running / last tool call, a recent-file
//! feed, and any pending approval. This module holds that fold.
//!
//! Design: [`docs/agent-panel-display.md`] §3. The registry is a global
//! `Entity<AgentRegistry>` (registered like [`crate::AppState`]). It lives in
//! `oneterm-state` — below both the feature UI crates and the shell — so the
//! Agent Panel stays feature-agnostic: `terminal-view` *feeds* the registry
//! (pushing events + grouping + lifecycle) and `agent-ui` *reads* it.
//!
//! `seq` dedup is already applied upstream (backend listeners, see
//! `oneterm_terminal::osc_agent::should_apply`); the registry keeps a
//! `last_seq` guard purely for defensiveness.

use gpui::{App, AppContext, Context, Entity, EntityId, Global};

use oneterm_terminal::{AgentState, AgentStatusEvent};

pub use crate::agent_model::{
    AgentCard, ApprovalInfo, FileEntry, Grouping, Lifecycle, ModelInfo, ToolRun,
};

// ── Summary counts (for the header) ─────────────────────────────────────

/// Aggregate state counts across all cards (spec §8 summary line).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentStateCounts {
    pub working: usize,
    pub blocked: usize,
    pub idle: usize,
    pub done: usize,
    pub error: usize,
    pub total: usize,
}

// ── The registry ────────────────────────────────────────────────────────

/// Global folded model behind the Agent Panel.
///
/// Cards are stored insertion-ordered (a card appears in the order its agent
/// first reported); grouping / ordering / filtering for display is applied by
/// the `agent-ui` view layer.
pub struct AgentRegistry {
    cards: Vec<AgentCard>,
    stale_threshold_ms: u64,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            stale_threshold_ms: 300_000,
        }
    }
}

/// Global wrapper for `Entity<AgentRegistry>`.
pub struct AgentRegistryGlobal(pub Entity<AgentRegistry>);

impl Global for AgentRegistryGlobal {}

impl AgentRegistry {
    /// Get the global `Entity<AgentRegistry>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AgentRegistryGlobal>().0.clone()
    }

    /// Get the global `Entity<AgentRegistry>` if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<AgentRegistryGlobal>().map(|g| g.0.clone())
    }

    /// Initialize the global registry.
    pub fn init(cx: &mut App) {
        if cx.try_global::<AgentRegistryGlobal>().is_none() {
            let entity = cx.new(|_| Self::default());
            cx.set_global(AgentRegistryGlobal(entity));
        }
    }

    /// All cards, insertion-ordered.
    pub fn cards(&self) -> &[AgentCard] {
        &self.cards
    }

    /// Whether there are no cards.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// The effective staleness threshold (ms).
    pub fn stale_threshold_ms(&self) -> u64 {
        self.stale_threshold_ms
    }

    /// Update the staleness threshold (from `UiConfig`). Notifies on change.
    pub fn set_stale_threshold_ms(&mut self, ms: u64, cx: &mut Context<Self>) {
        if self.stale_threshold_ms != ms {
            self.stale_threshold_ms = ms;
            cx.notify();
        }
    }

    /// Aggregate state counts across all cards (Ended counts as done).
    pub fn summary(&self) -> AgentStateCounts {
        let mut c = AgentStateCounts::default();
        for card in &self.cards {
            c.total += 1;
            if matches!(card.lifecycle, Lifecycle::Ended { .. }) {
                c.done += 1;
                continue;
            }
            match card.state {
                AgentState::Working => c.working += 1,
                AgentState::Blocked => c.blocked += 1,
                AgentState::Idle => c.idle += 1,
                AgentState::Done => c.done += 1,
                AgentState::Error => c.error += 1,
            }
        }
        c
    }

    /// Update the visible tab title for every card in a tab group.
    pub fn rename_tab_title(
        &mut self,
        tab_key: EntityId,
        tab_title: String,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.tab_key != tab_key || card.tab_title == tab_title {
                continue;
            }
            card.tab_title = tab_title.clone();
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Fold one event into the matching card (create if absent), refreshing its
    /// grouping metadata. `seq` dedup is already applied upstream.
    pub fn apply(
        &mut self,
        terminal_key: EntityId,
        grouping: Grouping,
        ev: &AgentStatusEvent,
        cx: &mut Context<Self>,
    ) {
        let agent_id = ev.agent();
        let idx = match self
            .cards
            .iter()
            .position(|c| c.terminal_key == terminal_key && c.agent_id == agent_id)
        {
            Some(i) => i,
            None => {
                self.cards.push(AgentCard::new(
                    terminal_key,
                    agent_id.to_string(),
                    &grouping,
                ));
                self.cards.len() - 1
            }
        };
        let card = &mut self.cards[idx];
        card.tab_key = grouping.tab_key;
        card.tab_title = grouping.tab_title;
        card.space_label = grouping.space_label;
        card.space_active = grouping.space_active;
        card.apply_event(ev);
        cx.notify();
    }

    /// Set the lifecycle of every card belonging to `terminal_key` (the host
    /// reports terminal exit/close — spec §5.2.7). Notifies on change.
    pub fn set_lifecycle(
        &mut self,
        terminal_key: EntityId,
        lifecycle: Lifecycle,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.terminal_key != terminal_key {
                continue;
            }
            // A later `Closed` (Ended{None}) must not clobber an exit code that a
            // preceding `Exited(code)` already recorded.
            let next = match (card.lifecycle, lifecycle) {
                (
                    Lifecycle::Ended {
                        exit_code: Some(existing),
                    },
                    Lifecycle::Ended { exit_code: None },
                ) => Lifecycle::Ended {
                    exit_code: Some(existing),
                },
                _ => lifecycle,
            };
            if card.lifecycle != next {
                card.lifecycle = next;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Drop every card for a terminal that was closed / dragged away.
    pub fn remove_terminal(&mut self, terminal_key: EntityId, cx: &mut Context<Self>) {
        let before = self.cards.len();
        self.cards.retain(|c| c.terminal_key != terminal_key);
        if self.cards.len() != before {
            cx.notify();
        }
    }

    /// Remove all ended cards (the ⚙ "Clear ended" action).
    pub fn clear_ended(&mut self, cx: &mut Context<Self>) {
        let before = self.cards.len();
        self.cards
            .retain(|c| !matches!(c.lifecycle, Lifecycle::Ended { .. }));
        if self.cards.len() != before {
            cx.notify();
        }
    }

    /// Low-frequency tick: mark `Live` cards `Stale` when no event has arrived
    /// within `max(stale_threshold, 3 × heartbeat_interval)` (spec §5.3). A
    /// threshold of `0` disables staleness marking.
    pub fn refresh_stale(&mut self, cx: &mut Context<Self>) {
        if self.stale_threshold_ms == 0 {
            return;
        }
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.lifecycle != Lifecycle::Live {
                continue;
            }
            let hb = card
                .heartbeat_interval
                .map(|i| i.saturating_mul(3))
                .unwrap_or(0);
            let effective_ms = self.stale_threshold_ms.max(hb);
            if card.last_recv.elapsed().as_millis() as u64 >= effective_ms {
                card.lifecycle = Lifecycle::Stale;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
}
