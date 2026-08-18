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
//! another feature. [`AgentListView`] is itself the right-dock panel.

mod card;
mod view;

use gpui::App;
use gpui_component::dock::register_panel;

use oneterm_state::{AgentRegistry, panel_names};

pub use view::AgentListView;

/// Initialize the Agent feature: ensure the `AgentRegistry` global exists (so
/// terminals can fold into it before the panel is first opened) and register the
/// [`panel_names::AGENT`] dock panel so the shell can build it by name and saved
/// layouts deserialize it.
pub fn init(cx: &mut App) {
    AgentRegistry::init(cx);
    register_panel(cx, panel_names::AGENT, |_, _, _, window, cx| {
        Box::new(AgentListView::new_entity(window, cx))
    });
}
