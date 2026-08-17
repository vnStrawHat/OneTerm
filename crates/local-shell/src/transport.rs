//! `LocalTransport` — the PTY-side [`PtyTransport`] for the local shell.
//!
//! Wraps the owner-thread [`ShellNotifier`] (created once the event loop
//! exists) so writes, resizes and shutdown flow through the bounded local
//! command queue. `LocalListener` is the shared [`OscRouter`] specialised to
//! this transport: both `Term<LocalListener>` and the event loop hold clones.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::WindowSize;

use oneterm_terminal::{OscRouter, PtyTransport, TerminalError};

use crate::event_loop::{ShellMsg, ShellNotifier};

/// Alacritty `EventListener` for the local shell (shared router + PTY transport).
pub(crate) type LocalListener = OscRouter<LocalTransport>;

/// PTY transport handle: routes writes/resize/shutdown to the owner thread.
#[derive(Clone, Default)]
pub(crate) struct LocalTransport {
    /// Set once the owner thread has constructed the event loop.
    notifier: Arc<Mutex<Option<ShellNotifier>>>,
}

fn map_notifier_error(error: std::io::Error) -> TerminalError {
    match error.kind() {
        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::NotConnected => TerminalError::Closed,
        std::io::ErrorKind::WouldBlock => TerminalError::QueueFull,
        _ => TerminalError::Transport(error.to_string()),
    }
}

impl LocalTransport {
    /// Create a transport with no notifier yet (every operation reports `Closed`).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Install the notifier once the owner thread has built the event loop.
    /// Can be called on any clone (Arc-shared).
    pub(crate) fn set_notifier(&self, notifier: ShellNotifier) {
        *self.notifier.lock().unwrap() = Some(notifier);
    }

    fn send(&self, msg: ShellMsg) -> Result<(), TerminalError> {
        let notifier = self
            .notifier
            .lock()
            .unwrap()
            .clone()
            .ok_or(TerminalError::Closed)?;
        notifier.send(msg).map_err(map_notifier_error)
    }
}

impl PtyTransport for LocalTransport {
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        self.send(ShellMsg::Input(Cow::Owned(bytes.to_vec())))
    }

    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.send(ShellMsg::Resize(WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 0,
            cell_height: 0,
        }))
    }

    fn pty_close(&self) -> Result<(), TerminalError> {
        self.send(ShellMsg::Shutdown)
    }
}
