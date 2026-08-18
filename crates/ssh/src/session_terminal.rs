//! `impl TerminalSession for SshSession` — the SSH capabilities, session kind
//! and channel teardown; everything else comes from the shared
//! `impl_pty_terminal_session!` in `oneterm-terminal` (ARCH-05).

use std::sync::Arc;

use oneterm_core::SftpBackend;
use oneterm_terminal::{
    PtyTransport, SessionKind, TerminalCapabilities, TerminalError, TerminalSession,
};

use crate::session::SshSession;
use crate::transport::SshListener;

oneterm_terminal::impl_pty_terminal_session!(
    SshSession,
    SshListener,
    "SshSession",
    SessionKind::Ssh,
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
                .unwrap()
                .clone()
                .map(|session| session as Arc<dyn SftpBackend>),
            cwd_source: Some(self.state.clone()),
        }
    }
}
