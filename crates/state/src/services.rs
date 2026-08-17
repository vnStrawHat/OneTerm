//! Application-scoped services installed by the composition root.
//!
//! Feature crates receive session creation, workspace commands and the
//! cross-feature hooks (active-terminal metrics, agent focus) through this one
//! bundle rather than independent process-global registries. Startup runs in
//! two phases: features *contribute* their hooks during `init()` through
//! [`AppServicesBuilder`], then the composition root seals the bundle with
//! [`AppServices::install`]. Test application contexts can install different
//! bundles without sharing process state.

use std::sync::Arc;

use gpui::{App, Global};
use oneterm_terminal::SessionFactory;

use crate::active_terminal::ActiveTerminalMetricsProvider;
use crate::agent_focus::AgentFocuser;
use crate::commands::WorkspaceCommands;

/// Feature contributions gathered during startup, before the bundle is sealed.
#[derive(Default)]
pub struct AppServicesBuilder {
    active_terminal_metrics: Option<ActiveTerminalMetricsProvider>,
    agent_focuser: Option<AgentFocuser>,
}

impl Global for AppServicesBuilder {}

impl AppServicesBuilder {
    /// The pending contributions of this application context, created on first use.
    ///
    /// Fails once [`AppServices::install`] has sealed the bundle: late feature
    /// registration is a startup ordering bug, not a runtime condition.
    pub fn pending(cx: &mut App) -> Result<&mut Self, &'static str> {
        if cx.has_global::<AppServices>() {
            return Err(
                "application services are already installed; features must contribute during init()",
            );
        }
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        Ok(cx.global_mut::<Self>())
    }

    /// Contribute the active-terminal metric extractors (terminal feature).
    pub fn active_terminal_metrics(
        &mut self,
        provider: ActiveTerminalMetricsProvider,
    ) -> Result<&mut Self, &'static str> {
        if self.active_terminal_metrics.is_some() {
            return Err("active-terminal metrics provider is already contributed");
        }
        self.active_terminal_metrics = Some(provider);
        Ok(self)
    }

    /// Contribute the agent focuser (terminal feature).
    pub fn agent_focuser(&mut self, focuser: AgentFocuser) -> Result<&mut Self, &'static str> {
        if self.agent_focuser.is_some() {
            return Err("agent focuser is already contributed");
        }
        self.agent_focuser = Some(focuser);
        Ok(self)
    }
}

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
    /// Takes the feature contributions gathered in [`AppServicesBuilder`] and
    /// rejects duplicate startup registration or a missing contribution.
    pub fn install(
        cx: &mut App,
        session_factory: Arc<dyn SessionFactory>,
        workspace_commands: WorkspaceCommands,
    ) -> Result<(), &'static str> {
        if cx.has_global::<Self>() {
            return Err("application services are already registered");
        }
        let builder = if cx.has_global::<AppServicesBuilder>() {
            cx.remove_global::<AppServicesBuilder>()
        } else {
            AppServicesBuilder::default()
        };
        let Some(active_terminal_metrics) = builder.active_terminal_metrics else {
            return Err("no feature contributed the active-terminal metrics provider");
        };
        let Some(agent_focuser) = builder.agent_focuser else {
            return Err("no feature contributed the agent focuser");
        };
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

    /// Validate that the composition-root bundle is installed.
    pub fn validate(cx: &App) -> Result<(), &'static str> {
        if !cx.has_global::<Self>() {
            return Err("application services are not registered");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_core::{LocalShellConfig, Result, SshConfig};
    use oneterm_terminal::{PtySize, TerminalSession};

    struct TestFactory;

    impl SessionFactory for TestFactory {
        fn spawn_local(
            &self,
            _: LocalShellConfig,
            _: PtySize,
            _: usize,
        ) -> Result<Box<dyn TerminalSession>> {
            Err(oneterm_core::AppError::msg("test factory"))
        }

        fn connect_ssh(
            &self,
            _: SshConfig,
            _: PtySize,
            _: usize,
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

    fn contribute_all(cx: &mut App) {
        AppServicesBuilder::pending(cx)
            .unwrap()
            .active_terminal_metrics(metrics())
            .unwrap()
            .agent_focuser(focuser())
            .unwrap();
    }

    #[gpui::test]
    fn contributions_are_sealed_into_the_installed_bundle(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            contribute_all(cx);
            AppServices::install(cx, Arc::new(TestFactory), commands()).unwrap();
            assert!(AppServices::validate(cx).is_ok());
            assert!(!cx.has_global::<AppServicesBuilder>());
            let provider = AppServices::active_terminal_metrics(cx);
            assert_eq!(provider.breadcrumb as usize, metrics().breadcrumb as usize);
        });
    }

    #[gpui::test]
    fn install_requires_every_feature_contribution(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            AppServicesBuilder::pending(cx)
                .unwrap()
                .active_terminal_metrics(metrics())
                .unwrap();
            assert_eq!(
                AppServices::install(cx, Arc::new(TestFactory), commands()),
                Err("no feature contributed the agent focuser")
            );
            assert!(AppServices::validate(cx).is_err());
        });
    }

    #[gpui::test]
    fn duplicate_contribution_and_late_contribution_are_rejected(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            contribute_all(cx);
            assert!(
                AppServicesBuilder::pending(cx)
                    .unwrap()
                    .agent_focuser(focuser())
                    .is_err()
            );
            AppServices::install(cx, Arc::new(TestFactory), commands()).unwrap();
            assert!(AppServicesBuilder::pending(cx).is_err());
            assert_eq!(
                AppServices::install(cx, Arc::new(TestFactory), commands()),
                Err("application services are already registered")
            );
        });
    }
}
