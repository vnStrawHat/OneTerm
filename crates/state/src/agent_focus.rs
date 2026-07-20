//! Injectable "focus the agent's terminal" hook.
//!
//! The Agent Panel (`agent-ui`, a feature crate) must be able to activate the
//! Tab/Space that hosts a clicked agent, but it may not depend on
//! `terminal-view` (crate rule R5). So `terminal-view` registers a focuser
//! function here at init (mirroring [`crate::active_terminal`]); the panel calls
//! [`focus_terminal`] with the card's `terminal_key`.
//!
//! The protocol is one-directional (agent → host; OSC 9;7 §6.3) — this uses
//! OneTerm's own dock / `SpaceTree::set_active` / focus APIs, never OSC.

use gpui::{App, EntityId, Global, Window};

/// Focuser function provided by the terminal feature crate.
#[derive(Clone, Copy)]
pub struct AgentFocuser {
    /// Activate the Tab (and, for a split Tab, the Space) hosting the terminal
    /// with `terminal_key`, then focus it. No-op if the terminal is gone.
    pub focus: fn(EntityId, &mut Window, &mut App),
}

impl Global for AgentFocuser {}

/// Register the focuser (called from the terminal feature's `init`).
pub fn set_focuser(cx: &mut App, focuser: AgentFocuser) {
    cx.set_global(focuser);
}

/// Focus the terminal hosting the agent with `terminal_key`. No-op if no
/// focuser is registered yet (e.g. very early startup).
pub fn focus_terminal(terminal_key: EntityId, window: &mut Window, cx: &mut App) {
    if let Some(focuser) = cx.try_global::<AgentFocuser>().copied() {
        (focuser.focus)(terminal_key, window, cx);
    }
}
