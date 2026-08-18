//! Application-scoped services installed by the composition root.
//!
//! Feature crates receive session creation, workspace commands and the
//! cross-feature hooks (active-terminal metrics, agent focus) through this one
//! bundle rather than independent process-global registries. The composition
//! root installs it once with every feature's contribution. Test application
//! contexts can install different bundles without sharing process state.

use std::sync::Arc;

use gpui::{App, Global};
use oneterm_terminal::SessionFactory;

use crate::active_terminal::ActiveTerminalMetricsProvider;
use crate::agent_focus::AgentFocuser;
use crate::commands::WorkspaceCommands;

/// Immutable service handles owned by one GPUI application context.
pub struct AppServices {
    session_factory: Arc<dyn SessionFactory>,
    workspace_commands: WorkspaceCommands,
    active_terminal_metrics: ActiveTerminalMetricsProvider,
    agent_focuser: AgentFocuser,
}

impl Global for AppServices {}

impl AppServices {
    /// Seal the service bundle at the application composition root.
    ///
    /// Rejects duplicate startup registration.
    pub fn install(
        cx: &mut App,
        session_factory: Arc<dyn SessionFactory>,
        workspace_commands: WorkspaceCommands,
        active_terminal_metrics: ActiveTerminalMetricsProvider,
        agent_focuser: AgentFocuser,
    ) -> Result<(), &'static str> {
        if cx.has_global::<Self>() {
            return Err("application services are already registered");
        }
        cx.set_global(Self {
            session_factory,
            workspace_commands,
            active_terminal_metrics,
            agent_focuser,
        });
        Ok(())
    }

    /// The installed service bundle.
    ///
    /// Startup invariant: the composition root installs the bundle before any
    /// window opens, so a missing bundle is a wiring bug and fails fast.
    pub fn global(cx: &App) -> &Self {
        cx.try_global::<Self>().unwrap_or_else(|| {
            panic!(
                "AppServices is not installed: the composition root must call \
                 AppServices::install before feature code or the shell runs"
            )
        })
    }

    /// Return the session factory for a feature operation.
    pub fn session_factory(cx: &App) -> Arc<dyn SessionFactory> {
        Arc::clone(&Self::global(cx).session_factory)
    }

    /// Return the workspace command callbacks for the shell.
    pub fn workspace_commands(cx: &App) -> WorkspaceCommands {
        Self::global(cx).workspace_commands
    }

    /// Return the active-terminal metric extractors for the status bar widgets.
    pub fn active_terminal_metrics(cx: &App) -> ActiveTerminalMetricsProvider {
        Self::global(cx).active_terminal_metrics
    }

    /// Return the agent focuser for the Agent Panel.
    pub fn agent_focuser(cx: &App) -> AgentFocuser {
        Self::global(cx).agent_focuser
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_core::{LocalShellConfig, Result, SshConfig};
    use oneterm_terminal::{PtySize, TerminalSecurityPolicy, TerminalSession};

    struct TestFactory;

    impl SessionFactory for TestFactory {
        fn spawn_local(
            &self,
            _: LocalShellConfig,
            _: PtySize,
            _: usize,
            _: TerminalSecurityPolicy,
        ) -> Result<Box<dyn TerminalSession>> {
            Err(oneterm_core::AppError::msg("test factory"))
        }

        fn connect_ssh(
            &self,
            _: SshConfig,
            _: PtySize,
            _: usize,
            _: TerminalSecurityPolicy,
        ) -> Result<Box<dyn TerminalSession>> {
            Err(oneterm_core::AppError::msg("test factory"))
        }
    }

    fn commands() -> WorkspaceCommands {
        fn terminal(
            _: oneterm_core::ShellKind,
            _: &mut gpui::Window,
            _: &mut gpui::App,
        ) -> std::sync::Arc<dyn gpui_component::dock::PanelView> {
            unreachable!()
        }
        fn window(_: &mut gpui::Window, _: &mut gpui::App) {}
        fn duplicate_ssh(
            _: oneterm_core::SshDuplicateConfig,
            _: Option<std::path::PathBuf>,
            _: crate::commands::SshDuplicateCompletion,
            _: &mut gpui::Window,
            _: &mut gpui::App,
        ) {
        }
        fn app(_: &mut gpui::App) {}
        fn dock(
            _: &gpui::Entity<gpui_component::dock::DockArea>,
            _: &mut gpui::Window,
            _: &mut gpui::App,
        ) {
        }
        WorkspaceCommands {
            new_terminal_with_shell: terminal,
            open_new_session_dialog: window,
            open_duplicate_ssh_dialog: duplicate_ssh,
            open_settings: app,
            open_about: window,
            find_in_active_terminal: dock,
            setup_key_bindings: app,
        }
    }

    fn metrics() -> ActiveTerminalMetricsProvider {
        fn breadcrumb(
            _: &gpui::Entity<gpui_component::dock::DockArea>,
            _: &gpui::App,
        ) -> Option<String> {
            Some("crumb".into())
        }
        fn net_stats(
            _: &gpui::Entity<gpui_component::dock::DockArea>,
            _: &gpui::App,
        ) -> Option<oneterm_terminal::NetStats> {
            None
        }
        ActiveTerminalMetricsProvider {
            breadcrumb,
            net_stats,
        }
    }

    fn focuser() -> AgentFocuser {
        fn focus(_: gpui::EntityId, _: &mut gpui::Window, _: &mut gpui::App) {}
        AgentFocuser { focus }
    }

    fn install(cx: &mut App) -> std::result::Result<(), &'static str> {
        AppServices::install(cx, Arc::new(TestFactory), commands(), metrics(), focuser())
    }

    #[gpui::test]
    fn contributions_are_readable_from_the_installed_bundle(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            install(cx).unwrap();
            let provider = AppServices::active_terminal_metrics(cx);
            assert_eq!(provider.breadcrumb as usize, metrics().breadcrumb as usize);
            assert_eq!(
                AppServices::agent_focuser(cx).focus as usize,
                focuser().focus as usize
            );
        });
    }

    #[gpui::test]
    fn duplicate_installation_is_rejected(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            install(cx).unwrap();
            assert_eq!(
                install(cx),
                Err("application services are already registered")
            );
        });
    }
}
