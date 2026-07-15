//! Session creation abstraction.
//!
//! UI feature crates create terminal sessions (local shell / SSH) **without**
//! depending on the backend crates (`oneterm-local-shell` / `oneterm-ssh`). Instead the
//! app installs a [`SessionFactory`] at startup via [`install_session_factory`];
//! feature crates create sessions through the process-global returned by
//! [`session_factory`]. This is what keeps the UI→backend dependency edge out of
//! the graph.

use std::sync::{Arc, OnceLock};

use oneterm_core::{LocalShellConfig, Result, SshConfig};

use crate::TerminalSession;

/// Initial PTY size (rows × cols).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of rows (lines).
    pub rows: u16,
    /// Number of columns.
    pub cols: u16,
}

/// Creates terminal sessions from connection parameters.
///
/// Implemented by the app crate (the only place that depends on both backend
/// crates) and installed as a process global via [`install_session_factory`].
/// UI feature crates call [`session_factory`] and never depend on the backends.
pub trait SessionFactory: Send + Sync + 'static {
    /// Spawn a local shell session.
    fn spawn_local(
        &self,
        cfg: LocalShellConfig,
        size: PtySize,
        scrollback: usize,
    ) -> Result<Box<dyn TerminalSession>>;

    /// Connect an SSH session (blocking — call on a background executor).
    fn connect_ssh(
        &self,
        cfg: SshConfig,
        size: PtySize,
        scrollback: usize,
    ) -> Result<Box<dyn TerminalSession>>;
}

static FACTORY: OnceLock<Arc<dyn SessionFactory>> = OnceLock::new();

/// Install the process-global session factory. Call once at startup (app init).
/// Subsequent calls are ignored (the first factory wins).
pub fn install_session_factory(factory: Arc<dyn SessionFactory>) {
    let _ = FACTORY.set(factory);
}

/// Get the installed session factory, or `None` if none has been installed yet.
pub fn session_factory() -> Option<Arc<dyn SessionFactory>> {
    FACTORY.get().cloned()
}
