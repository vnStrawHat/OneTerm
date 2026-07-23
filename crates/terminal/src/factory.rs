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

/// Isolated registration slot for a session factory.
///
/// Production uses one process-global slot, while tests can construct a fresh
/// slot per case and avoid mutating process-global state.
pub struct SessionFactorySlot {
    factory: OnceLock<Arc<dyn SessionFactory>>,
}

impl SessionFactorySlot {
    /// Create an empty factory slot.
    pub const fn new() -> Self {
        Self {
            factory: OnceLock::new(),
        }
    }

    /// Install a factory, rejecting duplicate registration.
    pub fn install(
        &self,
        factory: Arc<dyn SessionFactory>,
    ) -> std::result::Result<(), SessionFactoryAlreadyInstalled> {
        self.factory
            .set(factory)
            .map_err(|_| SessionFactoryAlreadyInstalled)
    }

    /// Return the installed factory, if any.
    pub fn get(&self) -> Option<Arc<dyn SessionFactory>> {
        self.factory.get().cloned()
    }
}

impl Default for SessionFactorySlot {
    fn default() -> Self {
        Self::new()
    }
}

static FACTORY: SessionFactorySlot = SessionFactorySlot::new();

/// Error returned when the process-global session factory is registered twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionFactoryAlreadyInstalled;

impl std::fmt::Display for SessionFactoryAlreadyInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("session factory is already installed")
    }
}

impl std::error::Error for SessionFactoryAlreadyInstalled {}

/// Install the process-global session factory. Call once at startup (app init).
/// Duplicate registration is rejected rather than silently retaining stale services.
pub fn install_session_factory(
    factory: Arc<dyn SessionFactory>,
) -> std::result::Result<(), SessionFactoryAlreadyInstalled> {
    FACTORY.install(factory)
}

/// Get the installed session factory, or `None` if none has been installed yet.
pub fn session_factory() -> Option<Arc<dyn SessionFactory>> {
    FACTORY.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFactory;

    impl SessionFactory for TestFactory {
        fn spawn_local(
            &self,
            _cfg: LocalShellConfig,
            _size: PtySize,
            _scrollback: usize,
        ) -> Result<Box<dyn TerminalSession>> {
            Err(oneterm_core::AppError::msg("unused test factory"))
        }

        fn connect_ssh(
            &self,
            _cfg: SshConfig,
            _size: PtySize,
            _scrollback: usize,
        ) -> Result<Box<dyn TerminalSession>> {
            Err(oneterm_core::AppError::msg("unused test factory"))
        }
    }

    #[test]
    fn isolated_slots_do_not_pollute_each_other() {
        let first = SessionFactorySlot::new();
        let second = SessionFactorySlot::new();
        assert!(first.get().is_none());
        assert!(second.get().is_none());
        first.install(Arc::new(TestFactory)).unwrap();
        assert!(first.get().is_some());
        assert!(second.get().is_none());
        assert_eq!(
            first.install(Arc::new(TestFactory)),
            Err(SessionFactoryAlreadyInstalled)
        );
    }
}
