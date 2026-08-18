//! [`TerminalDeps`] — the process-level services a terminal view needs,
//! resolved once by the panel that creates the view and passed into
//! [`LocalTerminalView::new`](super::LocalTerminalView::new) (ARCH-20) instead
//! of being reached through globals from every render and input handler.

use gpui::{App, Entity};

use oneterm_settings::TerminalSettings;
use oneterm_state::{AgentRegistry, CompletionHistory, GlobalCompletionHistory};

/// Explicit dependencies of one terminal view.
#[derive(Clone)]
pub(crate) struct TerminalDeps {
    /// Live terminal settings (font, colours, scrollback, security…).
    pub(crate) settings: Entity<TerminalSettings>,
    /// The Agent Panel model this terminal feeds; `None` when the agent
    /// feature is not initialised (tests, tools).
    pub(crate) agent_registry: Option<Entity<AgentRegistry>>,
    /// The cross-tab completion history; `None` when completion is not
    /// initialised.
    pub(crate) completion_history: Option<Entity<CompletionHistory>>,
}

impl TerminalDeps {
    /// Resolve the dependencies from the process globals. Called by the panel
    /// (the composition point for terminal views); the view itself never
    /// touches the globals again.
    pub(crate) fn from_globals(cx: &App) -> Self {
        Self {
            settings: TerminalSettings::global(cx),
            agent_registry: AgentRegistry::try_global(cx),
            completion_history: GlobalCompletionHistory::try_global(cx),
        }
    }
}
