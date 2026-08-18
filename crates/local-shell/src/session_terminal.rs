//! `impl TerminalSession for LocalSession` — the session kind and PTY
//! teardown; everything else comes from the shared
//! `impl_pty_terminal_session!` in `oneterm-terminal`.

use oneterm_terminal::{SessionKind, TerminalSession};

use crate::session::LocalSession;
use crate::transport::LocalListener;

oneterm_terminal::impl_pty_terminal_session!(
    LocalSession,
    LocalListener,
    "LocalSession",
    SessionKind::Local,
    shutdown_owner
);

impl TerminalSession for LocalSession {}
