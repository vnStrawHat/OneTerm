//! Session creation abstraction.
//!
//! UI feature crates create terminal sessions (local shell / SSH) **without**
//! depending on the backend crates (`oneterm-local-shell` / `oneterm-ssh`). The
//! application composition root installs a [`SessionFactory`] in its scoped
//! `AppServices` bundle; feature crates consume that handle through GPUI context.

use oneterm_core::{LocalShellConfig, Result, SshConfig};

use crate::TerminalSession;
use crate::security_policy::TerminalSecurityPolicy;

/// Initial PTY size (rows × cols).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    /// Number of rows (lines).
    pub rows: u16,
    /// Number of columns.
    pub cols: u16,
}

impl PtySize {
    /// Size a session is spawned with before its view reports the real grid
    /// (the classic 80×24 terminal); the first resize replaces it.
    pub const INITIAL: PtySize = PtySize { rows: 24, cols: 80 };
}

/// Creates terminal sessions from connection parameters.
///
/// Implemented by the app crate (the only place that depends on both backend
/// crates) and installed in the app-scoped `AppServices` bundle. UI feature
/// crates receive the handle through GPUI context and never depend on backends.
///
/// `security` is the user's [`TerminalSecurityPolicy`] (derived from the
/// terminal settings by the caller); the backend applies it to every
/// terminal-controlled side effect of the session (SEC-08).
pub trait SessionFactory: Send + Sync + 'static {
    /// Spawn a local shell session.
    fn spawn_local(
        &self,
        cfg: LocalShellConfig,
        size: PtySize,
        scrollback: usize,
        security: TerminalSecurityPolicy,
    ) -> Result<Box<dyn TerminalSession>>;

    /// Connect an SSH session (blocking — call on a background executor).
    fn connect_ssh(
        &self,
        cfg: SshConfig,
        size: PtySize,
        scrollback: usize,
        security: TerminalSecurityPolicy,
    ) -> Result<Box<dyn TerminalSession>>;
}
