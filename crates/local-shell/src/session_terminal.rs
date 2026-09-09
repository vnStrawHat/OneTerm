//! `impl TerminalSession for LocalSession` — the session kind and PTY
//! teardown; everything else comes from the shared
//! `impl_pty_terminal_session!` in `oneterm-terminal`.

use oneterm_terminal::{ResizePolicy, SessionKind, TerminalCapabilities, TerminalSession};

use crate::session::LocalSession;
use crate::transport::LocalListener;

/// ConPTY's conhost keeps its viewport top on a grow and addresses later output
/// in its own coordinates, so the grid must not pull scrollback (DEC-0008). Unix
/// PTYs leave the reflow to the shell, where alacritty's default is right.
const fn local_resize_policy() -> ResizePolicy {
    if cfg!(windows) {
        ResizePolicy::KeepViewportTop
    } else {
        ResizePolicy::Default
    }
}

oneterm_terminal::impl_pty_terminal_session!(
    LocalSession,
    LocalListener,
    "LocalSession",
    SessionKind::Local,
    local_resize_policy(),
    shutdown_owner
);

impl TerminalSession for LocalSession {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            logging: Some(self.listener.logging().clone()),
            ..TerminalCapabilities::default()
        }
    }
}
