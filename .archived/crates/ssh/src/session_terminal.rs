//! `impl PtyOwner for SshSession` — the channel writes, the teardown and the
//! SSH capabilities; everything else a `TerminalSession` does lives once in
//! `oneterm_terminal::PtySession` (ARCH-05).

use std::sync::Arc;

use oneterm_core::SftpBackend;
use oneterm_terminal::{PtyOwner, PtyTransport, TerminalCapabilities, TerminalError};

use crate::session::SshSession;

impl PtyOwner for SshSession {
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        self.transport().pty_write(bytes)
    }

    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.transport().pty_resize(rows, cols)
    }

    /// Close the SSH channel. SFTP shares the connection: closing the shell
    /// closes it too (ARCH-28).
    fn close(&self) -> Result<(), TerminalError> {
        let result = self.transport().pty_close();
        self.close_sftp();
        result
    }

    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            network_stats: Some(self.state.net_stats()),
            sftp: self
                .sftp
                .lock()
                // A poisoned lock must not take the panel down with it: the
                // handle behind it is still valid (error-policy.md), and
                // `SshSession::close_sftp` already reads it this way.
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .map(|session| session as Arc<dyn SftpBackend>),
            cwd_source: Some(self.state.clone()),
            logging: Some(self.listener.logging().clone()),
        }
    }
}
