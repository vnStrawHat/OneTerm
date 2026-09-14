//! `impl PtyOwner for LocalSession` — the PTY writes, the teardown and the one
//! capability a local shell offers; everything else a `TerminalSession` does
//! lives once in `oneterm_terminal::PtySession`.

use oneterm_terminal::{PtyOwner, PtyTransport, ResizePolicy, TerminalCapabilities, TerminalError};

use crate::session::LocalSession;

/// ConPTY's conhost keeps its viewport top on a grow and addresses later output
/// in its own coordinates, so the grid must not pull scrollback (DEC-0008). Unix
/// PTYs leave the reflow to the shell, where the engine's bottom-anchored
/// default is right.
pub(crate) const fn local_resize_policy() -> ResizePolicy {
    if cfg!(windows) {
        ResizePolicy::KeepViewportTop
    } else {
        ResizePolicy::BottomAnchor
    }
}

impl PtyOwner for LocalSession {
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        self.transport().pty_write(bytes)
    }

    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.transport().pty_resize(rows, cols)
    }

    fn close(&self) -> Result<(), TerminalError> {
        self.shutdown_owner()
    }

    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            logging: Some(self.listener.logging().clone()),
            ..TerminalCapabilities::default()
        }
    }
}
