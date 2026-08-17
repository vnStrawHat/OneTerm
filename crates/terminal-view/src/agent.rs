//! Agent Panel wiring owned by the terminal feature.
//!
//! Two responsibilities, both feeding `oneterm-state` so the Agent Panel
//! (`agent-ui`) stays feature-agnostic (see `docs/agent-panel-display.md` §12):
//!
//! 1. A per-terminal **navigation index** (`terminal_key → Tab/Space handles`),
//!    kept current every time a terminal reports an OSC 9;7 event.
//! 2. A **focuser** registered with [`oneterm_state::agent_focus`]: the panel
//!    calls it with a card's `terminal_key`; this activates the Tab (and, for a
//!    split Tab, the Space) and focuses the terminal — via OneTerm's own dock /
//!    `SpaceTree::set_active` / focus APIs, never OSC (the protocol is
//!    one-directional, spec §6.3).

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, EntityId, Global, WeakEntity, Window};

use gpui_component::dock::{PanelView, TabPanel};

use crate::TerminalPanel;
use crate::space::SpaceId;

/// The Tab/Space handles needed to navigate to a terminal hosting an agent.
#[derive(Clone)]
pub(crate) struct AgentNav {
    pub tab_panel: Option<WeakEntity<TabPanel>>,
    pub panel: WeakEntity<TerminalPanel>,
    pub space_id: SpaceId,
}

/// Global map `terminal_key → AgentNav`, updated on each OSC 9;7 event and
/// pruned when the terminal is shut down.
#[derive(Default)]
struct AgentNavIndex(HashMap<EntityId, AgentNav>);

impl Global for AgentNavIndex {}

/// Record / refresh the navigation entry for `terminal_key`.
pub(crate) fn register_nav(cx: &mut App, terminal_key: EntityId, nav: AgentNav) {
    if cx.try_global::<AgentNavIndex>().is_none() {
        cx.set_global(AgentNavIndex::default());
    }
    cx.global_mut::<AgentNavIndex>().0.insert(terminal_key, nav);
}

/// Drop the navigation entry for a closed terminal.
pub(crate) fn remove_nav(cx: &mut App, terminal_key: EntityId) {
    if cx.try_global::<AgentNavIndex>().is_some() {
        cx.global_mut::<AgentNavIndex>().0.remove(&terminal_key);
    }
}

/// Focuser implementation: activate the Tab (+ Space) and focus the terminal.
fn focus_agent_terminal(terminal_key: EntityId, window: &mut Window, cx: &mut App) {
    let Some(nav) = cx
        .try_global::<AgentNavIndex>()
        .and_then(|idx| idx.0.get(&terminal_key).cloned())
    else {
        return;
    };
    let Some(panel) = nav.panel.upgrade() else {
        return;
    };
    // 1. Select the agent's Tab within its TabPanel (reveals it if scrolled off).
    if let Some(tab_panel) = nav.tab_panel.as_ref().and_then(|w| w.upgrade()) {
        let arc: Arc<dyn PanelView> = Arc::new(panel.clone());
        tab_panel.update(cx, |tp, cx| tp.set_active_panel(&arc, window, cx));
    }
    // 2. Activate the agent's Space (single-Space tabs no-op past focus) and
    //    focus the terminal so keystrokes go to it.
    panel.update(cx, |p, cx| p.set_active_space(nav.space_id, window, cx));
}

/// Contribute the focuser to `AppServices` (called from [`crate::init`]).
pub fn init(cx: &mut App) {
    oneterm_state::AppServicesBuilder::pending(cx)
        .and_then(|builder| {
            builder.agent_focuser(oneterm_state::agent_focus::AgentFocuser {
                focus: focus_agent_terminal,
            })
        })
        .expect("terminal feature must contribute its agent focuser once during init");
}
