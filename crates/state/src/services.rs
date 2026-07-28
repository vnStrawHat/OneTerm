//! Application-scoped services installed by the composition root.
//!
//! Feature crates receive session creation and workspace commands through this
//! bundle rather than independent process-global registries. Test application
//! contexts can install different bundles without sharing process state.

use std::sync::Arc;

use gpui::{App, Global};
use oneterm_terminal::SessionFactory;

use crate::commands::WorkspaceCommands;

/// Immutable service handles owned by one GPUI application context.
pub struct AppServices {
    session_factory: Arc<dyn SessionFactory>,
    workspace_commands: WorkspaceCommands,
}

impl Global for AppServices {}

impl AppServices {
    /// Create a service bundle at the application composition root.
    pub fn new(
        session_factory: Arc<dyn SessionFactory>,
        workspace_commands: WorkspaceCommands,
    ) -> Self {
        Self {
            session_factory,
            workspace_commands,
        }
    }

    /// Install the service bundle, rejecting duplicate startup registration.
    pub fn install(self, cx: &mut App) -> Result<(), &'static str> {
        if cx.try_global::<Self>().is_some() {
            return Err("application services are already registered");
        }
        cx.set_global(self);
        Ok(())
    }

    /// Return the session factory for a feature operation.
    pub fn session_factory(cx: &App) -> Option<Arc<dyn SessionFactory>> {
        cx.try_global::<Self>()
            .map(|services| Arc::clone(&services.session_factory))
    }

    /// Return the workspace command callbacks for the shell.
    pub fn workspace_commands(cx: &App) -> Option<WorkspaceCommands> {
        cx.try_global::<Self>()
            .map(|services| services.workspace_commands)
    }

    /// Validate that all required composition-root services are installed.
    pub fn validate(cx: &App) -> Result<(), &'static str> {
        if cx.try_global::<Self>().is_none() {
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
            open_settings: app,
            open_about: window,
            find_in_active_terminal: dock,
            setup_key_bindings: app,
        }
    }

    #[gpui::test]
    fn independent_contexts_install_independent_service_bundles(first: &mut gpui::TestAppContext) {
        first.update(|cx| {
            AppServices::new(Arc::new(TestFactory), commands())
                .install(cx)
                .unwrap();
            assert!(AppServices::validate(cx).is_ok());
        });
    }
}
