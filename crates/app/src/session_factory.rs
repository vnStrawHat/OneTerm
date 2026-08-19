//! The concrete [`SessionFactory`] implementation.
//!
//! This is the only place that depends on both backend crates (`oneterm-local-shell`
//! + `oneterm-ssh`). It is installed as a process global at startup so the UI
//! feature crates can create sessions without a UI→backend dependency edge.

use std::sync::Arc;

use oneterm_core::{LocalShellConfig, Result, SshConfig, TerminalLogConfig};
use oneterm_terminal::{PtySize, SessionFactory, TerminalSecurityPolicy, TerminalSession};

/// App-owned factory that dispatches to the local + SSH backends.
struct AppSessionFactory;

impl SessionFactory for AppSessionFactory {
    fn spawn_local(
        &self,
        cfg: LocalShellConfig,
        size: PtySize,
        scrollback: usize,
        security: TerminalSecurityPolicy,
        logging: TerminalLogConfig,
    ) -> Result<Box<dyn TerminalSession>> {
        oneterm_local_shell::LocalSession::spawn(cfg, size, scrollback, security, logging)
            .map(|s| Box::new(s) as Box<dyn TerminalSession>)
    }

    fn connect_ssh(
        &self,
        cfg: SshConfig,
        size: PtySize,
        scrollback: usize,
        security: TerminalSecurityPolicy,
        logging: TerminalLogConfig,
    ) -> Result<Box<dyn TerminalSession>> {
        oneterm_ssh::connect(cfg, size, scrollback, security, logging)
    }
}

/// Build the app's session factory for the application service bundle.
pub(crate) fn build() -> Arc<dyn SessionFactory> {
    Arc::new(AppSessionFactory)
}
