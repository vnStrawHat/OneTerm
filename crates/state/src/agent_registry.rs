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

use std::collections::HashMap;

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
/// the `agent-ui` view layer. A composite `(terminal_key, agent_id)` index keeps
/// event folding constant-time while preserving that stable display order.
pub struct AgentRegistry {
    cards: Vec<AgentCard>,
    card_indices: HashMap<(EntityId, String), usize>,
    stale_threshold_ms: u64,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            card_indices: HashMap::new(),
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
        let key = (terminal_key, agent_id.to_string());
        let idx = match self.card_indices.get(&key).copied() {
            Some(index) => index,
            None => {
                self.cards
                    .push(AgentCard::new(terminal_key, key.1.clone(), &grouping));
                let index = self.cards.len() - 1;
                self.card_indices.insert(key, index);
                index
            }
        };
        let card = &mut self.cards[idx];
        card.tab_key = grouping.tab_key;
        card.tab_title = grouping.tab_title;
        card.space_number = grouping.space_number;
        card.space_order = grouping.space_order;
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
            self.rebuild_card_indices();
            cx.notify();
        }
    }

    /// Remove all ended cards (the ⚙ "Clear ended" action).
    pub fn clear_ended(&mut self, cx: &mut Context<Self>) {
        let before = self.cards.len();
        self.cards
            .retain(|c| !matches!(c.lifecycle, Lifecycle::Ended { .. }));
        if self.cards.len() != before {
            self.rebuild_card_indices();
            cx.notify();
        }
    }

    fn rebuild_card_indices(&mut self) {
        self.card_indices.clear();
        for (index, card) in self.cards.iter().enumerate() {
            self.card_indices
                .insert((card.terminal_key, card.agent_id.clone()), index);
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::TestAppContext;
    use oneterm_terminal::{AgentPayload, HeartbeatEvent, StateEvent};

    use super::*;

    fn grouping(tab_key: u64, tab_title: &str) -> Grouping {
        Grouping {
            tab_key: EntityId::from(tab_key),
            tab_title: tab_title.to_string(),
            space_number: 1,
            space_order: 0,
        }
    }

    fn state_event(agent: &str, seq: u64, state: AgentState) -> AgentStatusEvent {
        AgentStatusEvent {
            agent: agent.into(),
            seq,
            ts: seq * 1000,
            payload: AgentPayload::State(StateEvent {
                state,
                message: None,
                session_id: None,
            }),
        }
    }

    fn heartbeat(agent: &str, seq: u64, interval_ms: Option<u64>) -> AgentStatusEvent {
        AgentStatusEvent {
            agent: agent.into(),
            seq,
            ts: seq * 1000,
            payload: AgentPayload::Heartbeat(HeartbeatEvent {
                interval_ms,
                state: None,
            }),
        }
    }

    fn registry(cx: &mut TestAppContext) -> Entity<AgentRegistry> {
        cx.update(|cx| cx.new(|_| AgentRegistry::default()))
    }

    fn apply_state(
        registry: &mut AgentRegistry,
        terminal: EntityId,
        grouping: Grouping,
        agent: &str,
        seq: u64,
        state: AgentState,
        cx: &mut Context<AgentRegistry>,
    ) {
        registry.apply(terminal, grouping, &state_event(agent, seq, state), cx);
    }

    #[gpui::test]
    fn apply_creates_one_card_per_terminal_and_agent_and_refreshes_grouping(
        cx: &mut TestAppContext,
    ) {
        let registry = registry(cx);
        let first = EntityId::from(1u64);
        let second = EntityId::from(2u64);
        registry.update(cx, |registry, cx| {
            apply_state(
                registry,
                first,
                grouping(10, "tab-a"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            apply_state(
                registry,
                first,
                grouping(10, "tab-a"),
                "pi",
                2,
                AgentState::Blocked,
                cx,
            );
            apply_state(
                registry,
                second,
                grouping(20, "tab-b"),
                "pi",
                1,
                AgentState::Idle,
                cx,
            );
            apply_state(
                registry,
                first,
                grouping(11, "tab-a2"),
                "qa",
                1,
                AgentState::Done,
                cx,
            );
        });
        registry.read_with(cx, |registry, _| {
            let cards = registry.cards();
            assert_eq!(cards.len(), 3, "same (terminal, agent) folds into one card");
            assert_eq!(cards[0].agent_id, "pi");
            assert_eq!(cards[0].state, AgentState::Blocked);
            assert_eq!(cards[0].last_seq, 2);
            assert_eq!(cards[1].terminal_key, second);
            // Grouping metadata follows the latest event.
            assert_eq!(cards[2].tab_key, EntityId::from(11u64));
            assert_eq!(cards[2].tab_title, "tab-a2");
        });
    }

    #[gpui::test]
    fn set_lifecycle_keeps_a_recorded_exit_code_over_a_later_close(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let terminal = EntityId::from(1u64);
        let other = EntityId::from(2u64);
        registry.update(cx, |registry, cx| {
            apply_state(
                registry,
                terminal,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            apply_state(
                registry,
                other,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            registry.set_lifecycle(terminal, Lifecycle::Ended { exit_code: Some(3) }, cx);
            registry.set_lifecycle(terminal, Lifecycle::Ended { exit_code: None }, cx);
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(
                registry.cards()[0].lifecycle,
                Lifecycle::Ended { exit_code: Some(3) }
            );
            assert_eq!(
                registry.cards()[1].lifecycle,
                Lifecycle::Live,
                "other terminals untouched"
            );
        });
    }

    #[gpui::test]
    fn remove_terminal_and_clear_ended_rebuild_the_index(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let first = EntityId::from(1u64);
        let second = EntityId::from(2u64);
        let third = EntityId::from(3u64);
        registry.update(cx, |registry, cx| {
            apply_state(
                registry,
                first,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            apply_state(
                registry,
                second,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            apply_state(
                registry,
                third,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            registry.remove_terminal(first, cx);
            registry.set_lifecycle(second, Lifecycle::Ended { exit_code: Some(0) }, cx);
            registry.clear_ended(cx);
            // The surviving card must still fold by its composite key.
            apply_state(
                registry,
                third,
                grouping(10, "tab"),
                "pi",
                2,
                AgentState::Done,
                cx,
            );
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(registry.cards().len(), 1);
            assert_eq!(registry.cards()[0].terminal_key, third);
            assert_eq!(registry.cards()[0].state, AgentState::Done);
            assert_eq!(registry.card_indices.len(), 1);
            assert_eq!(
                registry.card_indices.get(&(third, "pi".to_string())),
                Some(&0)
            );
        });
    }

    #[gpui::test]
    fn refresh_stale_marks_live_cards_after_the_effective_window(cx: &mut TestAppContext) {
        let registry = registry(cx);
        let quiet = EntityId::from(1u64);
        let heartbeating = EntityId::from(2u64);
        let ended = EntityId::from(3u64);
        registry.update(cx, |registry, cx| {
            registry.set_stale_threshold_ms(1_000, cx);
            apply_state(
                registry,
                quiet,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            // 3 x heartbeat interval (30 s) exceeds the 1 s threshold.
            registry.apply(
                heartbeating,
                grouping(10, "tab"),
                &heartbeat("pi", 1, Some(10_000)),
                cx,
            );
            apply_state(
                registry,
                ended,
                grouping(10, "tab"),
                "pi",
                1,
                AgentState::Working,
                cx,
            );
            registry.set_lifecycle(ended, Lifecycle::Ended { exit_code: None }, cx);
            let stale_since = Instant::now() - Duration::from_secs(5);
            for card in registry.cards.iter_mut() {
                card.last_recv = stale_since;
            }
            registry.refresh_stale(cx);
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(registry.cards()[0].lifecycle, Lifecycle::Stale);
            assert_eq!(registry.cards()[1].lifecycle, Lifecycle::Live);
            assert_eq!(
                registry.cards()[2].lifecycle,
                Lifecycle::Ended { exit_code: None }
            );
        });

        // A threshold of 0 disables staleness marking entirely.
        registry.update(cx, |registry, cx| {
            registry.set_stale_threshold_ms(0, cx);
            registry.cards[1].last_recv = Instant::now() - Duration::from_secs(3600);
            registry.refresh_stale(cx);
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(registry.cards()[1].lifecycle, Lifecycle::Live);
        });
    }

    #[gpui::test]
    fn summary_counts_states_and_treats_ended_as_done(cx: &mut TestAppContext) {
        let registry = registry(cx);
        registry.update(cx, |registry, cx| {
            let states = [
                AgentState::Working,
                AgentState::Working,
                AgentState::Blocked,
                AgentState::Idle,
                AgentState::Done,
                AgentState::Error,
            ];
            for (index, state) in states.into_iter().enumerate() {
                let terminal = EntityId::from(index as u64 + 1);
                apply_state(registry, terminal, grouping(10, "tab"), "pi", 1, state, cx);
            }
            // An ended Working card counts as done, not working.
            registry.set_lifecycle(
                EntityId::from(1u64),
                Lifecycle::Ended { exit_code: Some(0) },
                cx,
            );
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(
                registry.summary(),
                AgentStateCounts {
                    working: 1,
                    blocked: 1,
                    idle: 1,
                    done: 2,
                    error: 1,
                    total: 6,
                }
            );
        });
    }

    #[gpui::test]
    fn rename_tab_title_updates_only_the_tab_group(cx: &mut TestAppContext) {
        let registry = registry(cx);
        registry.update(cx, |registry, cx| {
            let first = EntityId::from(1u64);
            let second = EntityId::from(2u64);
            apply_state(
                registry,
                first,
                grouping(10, "old"),
                "pi",
                1,
                AgentState::Idle,
                cx,
            );
            apply_state(
                registry,
                second,
                grouping(20, "other"),
                "pi",
                1,
                AgentState::Idle,
                cx,
            );
            registry.rename_tab_title(EntityId::from(10u64), "new".into(), cx);
        });
        registry.read_with(cx, |registry, _| {
            assert_eq!(registry.cards()[0].tab_title, "new");
            assert_eq!(registry.cards()[1].tab_title, "other");
        });
    }

    fn card(terminal_key: EntityId, agent_id: &str) -> AgentCard {
        AgentCard::new(
            terminal_key,
            agent_id.to_string(),
            &Grouping {
                tab_key: EntityId::from(99u64),
                tab_title: "tab".to_string(),
                space_number: 0,
                space_order: 0,
            },
        )
    }

    #[test]
    fn composite_index_tracks_stable_card_positions() {
        let first = EntityId::from(1u64);
        let second = EntityId::from(2u64);
        let mut registry = AgentRegistry::default();
        registry.cards = vec![card(first, "agent-a"), card(second, "agent-b")];
        registry.rebuild_card_indices();

        assert_eq!(
            registry.card_indices.get(&(first, "agent-a".to_string())),
            Some(&0)
        );
        assert_eq!(
            registry.card_indices.get(&(second, "agent-b".to_string())),
            Some(&1)
        );
    }
}
