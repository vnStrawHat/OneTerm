//! `impl TerminalSession for SshSession` — the SSH capabilities, session kind
//! and channel teardown; everything else comes from the shared
//! `impl_pty_terminal_session!` in `oneterm-terminal` (ARCH-05).

use std::sync::Arc;

use oneterm_core::SftpBackend;
use oneterm_terminal::{
    PtyTransport, SessionKind, TerminalCapabilities, TerminalError, TerminalSession,
};

use crate::session::SshSession;

oneterm_terminal::impl_pty_terminal_session!(
    SshSession,
    "SshSession",
    SessionKind::Ssh,
    // `oneterm_vt::ResizePolicy::BottomAnchor` is what reaches `Terminal::resize`
    // (DEC-0008: the remote PTY reflows and repaints on its side, so a row grow
    // pulls scrollback into the viewport top and the cursor follows it down).
    // The engine value cannot be named here yet — `impl_pty_terminal_session!`
    // declares `resize_policy()` as returning the adapter enum; see `US-0084`'s
    // packet, gap 2.
    oneterm_terminal::ResizePolicy::Default,
    close_channel
);

impl SshSession {
    /// Close the SSH channel. SFTP shares the connection: closing the shell
    /// closes it too (ARCH-28).
    fn close_channel(&self) -> Result<(), TerminalError> {
        let result = self.transport().pty_close();
        self.close_sftp();
        result
    }
}

impl TerminalSession for SshSession {
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
