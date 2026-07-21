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

use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, FocusHandle, Focusable, Task};

use oneterm_settings::UiConfig;
use oneterm_state::{AgentCard, AgentRegistry, Lifecycle};
use oneterm_terminal::AgentState;

/// How often the view re-renders relative-time labels and card spinners.
const RELATIVE_TIME_TICK: Duration = Duration::from_millis(120);

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

/// The Agent Panel content view. Observes the global [`AgentRegistry`] and
/// re-renders on registry changes and on a periodic tick for relative-time
/// labels; owns the view-local filter state.
pub struct AgentListView {
    focus_handle: FocusHandle,
    filter: Filter,
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

        let mut subs = Vec::new();
        subs.push(cx.observe(&registry, |_, _, cx| cx.notify()));
        subs.push(cx.observe(&ui, |_, ui, cx| {
            let ms = ui.read(cx).agent_stale_threshold_ms();
            AgentRegistry::global(cx).update(cx, |reg, cx| reg.set_stale_threshold_ms(ms, cx));
        }));

        let refresh_task = cx.spawn(async move |this, cx| {
            let mut stale_elapsed = Duration::ZERO;
            loop {
                cx.background_executor().timer(RELATIVE_TIME_TICK).await;
                stale_elapsed += RELATIVE_TIME_TICK;

                let alive = this
                    .update(cx, |_, cx| {
                        if stale_elapsed >= STALE_TICK {
                            stale_elapsed = Duration::ZERO;
                            if let Some(reg) = AgentRegistry::try_global(cx) {
                                reg.update(cx, |reg, cx| reg.refresh_stale(cx));
                            }
                        }

                        // Relative-time labels are derived from `Instant::elapsed()`
                        // during render, so they need a view refresh even when no
                        // registry state changed.
                        cx.notify();
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
