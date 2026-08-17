//! Injectable "focus the agent's terminal" hook.
//!
//! The Agent Panel (`agent-ui`, a feature crate) must be able to activate the
//! Tab/Space that hosts a clicked agent, but it may not depend on
//! `terminal-view` (crate rule R5). So `terminal-view` contributes a focuser
//! function to [`crate::AppServices`] at init (mirroring
//! [`crate::active_terminal`]); the panel calls [`focus_terminal`] with the
//! card's `terminal_key`.
//!
//! The protocol is one-directional (agent → host; OSC 9;7 §6.3) — this uses
//! OneTerm's own dock / `SpaceTree::set_active` / focus APIs, never OSC.

use gpui::{App, EntityId, Window};

use crate::AppServices;

/// Focuser function provided by the terminal feature crate.
#[derive(Clone, Copy)]
pub struct AgentFocuser {
    /// Activate the Tab (and, for a split Tab, the Space) hosting the terminal
    /// with `terminal_key`, then focus it. No-op if the terminal is gone.
    pub focus: fn(EntityId, &mut Window, &mut App),
}

/// Focus the terminal hosting the agent with `terminal_key`.
pub fn focus_terminal(terminal_key: EntityId, window: &mut Window, cx: &mut App) {
    (AppServices::agent_focuser(cx).focus)(terminal_key, window, cx);
}
