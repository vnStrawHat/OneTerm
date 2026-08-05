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

use gpui::App;

use oneterm_state::AgentRegistry;

pub use view::AgentListView;

/// Initialize the Agent feature. Ensures the `AgentRegistry` global exists so
/// terminals can fold into it even before the panel is first opened.
pub fn init(cx: &mut App) {
    AgentRegistry::init(cx);
}
