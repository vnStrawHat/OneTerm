//! OneTerm Agent feature — the **Agent Panel** content.
//!
//! A right-dock "fleet view" of coding agents running inside terminals. Agents
//! report status over OSC 9;7 (`docs/osc-agent-status.md`); `terminal-view`
//! folds those events into a global [`oneterm_state::AgentRegistry`], and this
//! crate renders it: a scrolling column of tab groups, each holding one card per
//! `(terminal, agent)`. See `docs/agent-panel-display.md`.
//!
//! Layering: this is a **feature crate** (crate rule R5) — it depends only on
//! shared layers (`state`, `terminal`, `settings`, `theme`, `ui`) and never on
//! another feature. The `app` crate composes [`AgentListView`] into the dock
//! panel (R9), mirroring how `SshClientPanel` composes `SessionPanel`.

mod card;
mod view;

use std::{collections::HashMap, time::Duration};

use gpui::{App, AppContext as _, Context, Entity, FocusHandle, Focusable, Task};

use oneterm_settings::UiConfig;
use oneterm_state::{AgentCard, AgentRegistry, Lifecycle};
use oneterm_terminal::AgentState;

/// How often active cards advance their spinner animation.
const ACTIVE_CARD_TICK: Duration = Duration::from_millis(120);

/// How often inactive cards refresh their relative-time labels.
const RELATIVE_TIME_TICK: Duration = Duration::from_secs(1);

/// How often the registry is polled to mark idle cards stale (§9).
const STALE_TICK: Duration = Duration::from_secs(15);

/// Card-state filter for the header chips (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Working,
    Blocked,
    Errors,
    Idle,
    Done,
}

/// Stable display order for one terminal group, rebuilt only on registry changes.
struct AgentDisplayGroup {
    tab_key: gpui::EntityId,
    tab_title: String,
    card_indices: Vec<usize>,
}

fn build_display_groups(cards: &[AgentCard]) -> Vec<AgentDisplayGroup> {
    let mut group_indices = HashMap::new();
    let mut groups: Vec<AgentDisplayGroup> = Vec::new();
    for (card_index, card) in cards.iter().enumerate() {
        let group_index = match group_indices.get(&card.tab_key).copied() {
            Some(index) => index,
            None => {
                let index = groups.len();
                group_indices.insert(card.tab_key, index);
                groups.push(AgentDisplayGroup {
                    tab_key: card.tab_key,
                    tab_title: card.tab_title.clone(),
                    card_indices: Vec::new(),
                });
                index
            }
        };
        groups[group_index].card_indices.push(card_index);
    }
    for group in &mut groups {
        group.card_indices.sort_by(|&a, &b| {
            cards[a]
                .sort_rank()
                .cmp(&cards[b].sort_rank())
                .then(cards[b].last_recv.cmp(&cards[a].last_recv))
        });
    }
    groups
}

/// The Agent Panel content view. Observes the global [`AgentRegistry`] and
/// re-renders on registry changes and on a periodic tick for relative-time
/// labels; owns the view-local filter state.
pub struct AgentListView {
    focus_handle: FocusHandle,
    filter: Filter,
    /// Cached registry data. Animation ticks must not clone the registry.
    cards: Vec<AgentCard>,
    groups: Vec<AgentDisplayGroup>,
    counts: oneterm_state::AgentStateCounts,
    has_working_cards: bool,
    _subs: Vec<gpui::Subscription>,
    _refresh_task: Task<()>,
}

impl AgentListView {
    /// Create the view: ensure the registry exists, wire the config-driven stale
    /// threshold, and start the periodic relative-time / stale refresh tick.
    pub fn new(_window: &mut gpui::Window, cx: &mut Context<Self>) -> Self {
        AgentRegistry::init(cx);
        let registry = AgentRegistry::global(cx);

        let ui = UiConfig::global(cx);
        let threshold = ui.read(cx).agent_stale_threshold_ms();
        registry.update(cx, |reg, cx| reg.set_stale_threshold_ms(threshold, cx));

        let (cards, counts) = {
            let reg = registry.read(cx);
            (reg.cards().to_vec(), reg.summary())
        };
        let has_working_cards = cards.iter().any(|card| card.state == AgentState::Working);
        let groups = build_display_groups(&cards);

        let mut subs = Vec::new();
        subs.push(cx.observe(&registry, |this, registry, cx| {
            // Clone the model only when the registry changes. Animation and
            // relative-time ticks render the cached snapshot below.
            let (cards, counts) = {
                let reg = registry.read(cx);
                (reg.cards().to_vec(), reg.summary())
            };
            this.cards = cards;
            this.has_working_cards = this
                .cards
                .iter()
                .any(|card| this.passes_filter(card) && card.state == AgentState::Working);
            this.groups = build_display_groups(&this.cards);
            this.counts = counts;
            cx.notify();
        }));
        subs.push(cx.observe(&ui, |_, ui, cx| {
            let ms = ui.read(cx).agent_stale_threshold_ms();
            AgentRegistry::global(cx).update(cx, |reg, cx| reg.set_stale_threshold_ms(ms, cx));
        }));

        let refresh_task = cx.spawn(async move |this, cx| {
            let mut stale_elapsed = Duration::ZERO;
            let mut relative_elapsed = Duration::ZERO;
            loop {
                cx.background_executor().timer(ACTIVE_CARD_TICK).await;
                stale_elapsed += ACTIVE_CARD_TICK;
                relative_elapsed += ACTIVE_CARD_TICK;

                let alive = this
                    .update(cx, |this, cx| {
                        if stale_elapsed >= STALE_TICK {
                            stale_elapsed = Duration::ZERO;
                            if let Some(reg) = AgentRegistry::try_global(cx) {
                                reg.update(cx, |reg, cx| reg.refresh_stale(cx));
                            }
                        }

                        // Working cards need the animation cadence. Inactive cards
                        // need only the coarser relative-time refresh cadence.
                        let refresh_relative = relative_elapsed >= RELATIVE_TIME_TICK;
                        if refresh_relative {
                            relative_elapsed = Duration::ZERO;
                        }
                        if this.has_working_cards || refresh_relative {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            filter: Filter::All,
            cards,
            groups,
            counts,
            has_working_cards,
            _subs: subs,
            _refresh_task: refresh_task,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut gpui::Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn passes_filter(&self, card: &AgentCard) -> bool {
        if matches!(card.lifecycle, Lifecycle::Ended { .. }) {
            return false;
        }
        match self.filter {
            Filter::All => true,
            Filter::Working => card.state == AgentState::Working,
            Filter::Blocked => card.state == AgentState::Blocked,
            Filter::Errors => card.state == AgentState::Error,
            Filter::Idle => card.state == AgentState::Idle,
            Filter::Done => card.state == AgentState::Done,
        }
    }
}

impl Focusable for AgentListView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Initialize the Agent feature. Ensures the `AgentRegistry` global exists so
/// terminals can fold into it even before the panel is first opened.
pub fn init(cx: &mut App) {
    AgentRegistry::init(cx);
}
