//! Agent Panel wiring owned by the terminal feature.
//!
//! Two responsibilities, both feeding `oneterm-state` so the Agent Panel
//! (`agent-ui`) stays feature-agnostic (see `docs/agent-panel-display.md` §12):
//!
//! 1. Building the per-terminal **navigation target** ([`agent_nav`]) that the
//!    view stores in the [`oneterm_state::AgentRegistry`] with every OSC 9;7
//!    event (ARCH-13: the registry owns the index; there is no separate
//!    mutable global).
//! 2. A **focuser** registered with [`oneterm_state::agent_focus`]: the panel
//!    calls it with a card's `terminal_key`; this looks the target up in the
//!    registry and runs it — activating the Tab (and, for a split Tab, the
//!    Space) and focusing the terminal via OneTerm's own dock /
//!    `SpaceTree::set_active` / focus APIs, never OSC (the protocol is
//!    one-directional, spec §6.3).

use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, EntityId, WeakEntity, Window};

use gpui_component::dock::{PanelView, TabPanel};
use oneterm_state::{AgentNav, AgentRegistry};

use crate::TerminalPanel;
use crate::space::SpaceId;

/// Build the navigation target for the terminal in Space `space_id` of
/// `panel` (hosted by `tab_panel`).
pub(crate) fn agent_nav(
    tab_panel: Option<WeakEntity<TabPanel>>,
    panel: WeakEntity<TerminalPanel>,
    space_id: SpaceId,
) -> AgentNav {
    Rc::new(move |window: &mut Window, cx: &mut App| {
        let Some(panel) = panel.upgrade() else {
            return;
        };
        // 1. Select the agent's Tab within its TabPanel (reveals it if scrolled off).
        if let Some(tab_panel) = tab_panel.as_ref().and_then(|w| w.upgrade()) {
            let arc: Arc<dyn PanelView> = Arc::new(panel.clone());
            tab_panel.update(cx, |tp, cx| tp.set_active_panel(&arc, window, cx));
        }
        // 2. Activate the agent's Space (single-Space tabs no-op past focus) and
        //    focus the terminal so keystrokes go to it.
        panel.update(cx, |p, cx| p.set_active_space(space_id, window, cx));
    })
}

/// Focuser implementation: run the registry's navigation target for the terminal.
fn focus_agent_terminal(terminal_key: EntityId, window: &mut Window, cx: &mut App) {
    let Some(nav) = AgentRegistry::try_global(cx).and_then(|reg| reg.read(cx).nav(terminal_key))
    else {
        return;
    };
    nav(window, cx);
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
