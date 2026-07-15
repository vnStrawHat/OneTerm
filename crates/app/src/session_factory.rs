//! The concrete [`SessionFactory`] implementation.
//!
//! This is the only place that depends on both backend crates (`oneterm-local-shell`
//! + `oneterm-ssh`). It is installed as a process global at startup so the UI
//! feature crates can create sessions without a UI→backend dependency edge.

use std::sync::Arc;

use oneterm_core::{LocalShellConfig, Result, SshConfig};
use oneterm_terminal::{PtySize, SessionFactory, TerminalSession, install_session_factory};

/// App-owned factory that dispatches to the local + SSH backends.
struct AppSessionFactory;

impl SessionFactory for AppSessionFactory {
    fn spawn_local(
        &self,
        cfg: LocalShellConfig,
        size: PtySize,
        scrollback: usize,
    ) -> Result<Box<dyn TerminalSession>> {
        oneterm_local_shell::LocalSession::spawn(cfg, size, scrollback)
            .map(|s| Box::new(s) as Box<dyn TerminalSession>)
    }

    fn connect_ssh(
        &self,
        cfg: SshConfig,
        size: PtySize,
        scrollback: usize,
    ) -> Result<Box<dyn TerminalSession>> {
        oneterm_ssh::connect(cfg, size, scrollback)
    }
}

/// Install the app's session factory. Call once at startup, before the UI can
/// create any terminal session.
pub fn install() {
    install_session_factory(Arc::new(AppSessionFactory));
}
