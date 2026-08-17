//! `SshTransport` — the channel-side [`PtyTransport`] for the SSH session.
//!
//! Bridges the sync UI thread to the tokio task through a bounded
//! `async_channel` of [`Cmd`]s: writes reserve from a 4 MiB byte budget and are
//! FIFO, resizes coalesce to the latest size, close sets an out-of-band flag so
//! it is honoured even when the queue is full. `SshListener` is the shared
//! `OscRouter` specialised to this transport.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(any(test, feature = "terminal-diagnostics"))]
use std::sync::atomic::AtomicU64;

use async_channel::{Sender, TrySendError};

use oneterm_terminal::{OscRouter, PtyTransport, TerminalError};

/// Alacritty `EventListener` for the SSH session (shared router + SSH transport).
pub(crate) type SshListener = OscRouter<SshTransport>;

/// Maximum queued SSH command messages.
pub(crate) const SSH_COMMAND_QUEUE_CAPACITY: usize = 256;
/// Maximum aggregate payload bytes waiting for SSH transport delivery.
pub(crate) const SSH_COMMAND_BYTE_BUDGET: usize = 4 * 1024 * 1024;

/// Command sent from the main thread → tokio task (via async_channel).
#[derive(Debug)]
pub(crate) enum Cmd {
    /// Write bytes to the SSH channel (keystroke, paste, OSC response).
    Write(Vec<u8>),
    /// Apply the latest coalesced PTY size.
    Resize,
    /// Close the channel.
    Close,
}

#[derive(Default)]
struct PendingResize {
    latest: Option<(u16, u16)>,
    signal_enqueued: bool,
}

/// Snapshot of SSH command-queue failures (diagnostics and tests).
#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SshCommandDiagnostics {
    /// Command writes rejected because the command queue was full.
    pub(crate) command_full: u64,
    /// Command writes rejected because the command queue was closed.
    pub(crate) command_closed: u64,
    /// Aggregate write payload bytes currently queued or in flight.
    pub(crate) queued_write_bytes: usize,
}

#[cfg(any(test, feature = "terminal-diagnostics"))]
#[derive(Default)]
struct CommandCounters {
    command_full: AtomicU64,
    command_closed: AtomicU64,
}

/// SSH channel transport handle (Arc-shared between `Term`, the session and
/// the tokio task).
#[derive(Clone)]
pub(crate) struct SshTransport {
    /// Channel sending `Cmd` to the tokio task (sync→async bridge).
    cmd_tx: Sender<Cmd>,
    /// Set when close has been requested — the tokio task checks this flag so
    /// close is always honoured even if `Cmd::Close` was dropped.
    closing: Arc<AtomicBool>,
    /// Aggregate bytes reserved by queued or in-flight `Cmd::Write` messages.
    queued_write_bytes: Arc<AtomicUsize>,
    /// Latest resize and whether a queue wakeup marker is already pending.
    pending_resize: Arc<Mutex<PendingResize>>,
    /// Diagnostic counters for bounded queue failures.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    counters: Arc<CommandCounters>,
}

impl SshTransport {
    /// Wrap the command sender consumed by `ssh_main_task`.
    pub(crate) fn new(cmd_tx: Sender<Cmd>) -> Self {
        Self {
            cmd_tx,
            closing: Arc::new(AtomicBool::new(false)),
            queued_write_bytes: Arc::new(AtomicUsize::new(0)),
            pending_resize: Arc::new(Mutex::new(PendingResize::default())),
            #[cfg(any(test, feature = "terminal-diagnostics"))]
            counters: Arc::new(CommandCounters::default()),
        }
    }

    /// Return the command-queue failure counters.
    #[cfg(any(test, feature = "terminal-diagnostics"))]
    pub(crate) fn diagnostics(&self) -> SshCommandDiagnostics {
        SshCommandDiagnostics {
            command_full: self.counters.command_full.load(Ordering::Relaxed),
            command_closed: self.counters.command_closed.load(Ordering::Relaxed),
            queued_write_bytes: self.queued_write_bytes.load(Ordering::Relaxed),
        }
    }

    #[cfg(any(test, feature = "terminal-diagnostics"))]
    fn record_failure<T>(&self, error: &TrySendError<T>) {
        let counter = match error {
            TrySendError::Full(_) => &self.counters.command_full,
            TrySendError::Closed(_) => &self.counters.command_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(not(any(test, feature = "terminal-diagnostics")))]
    fn record_failure<T>(&self, _error: &TrySendError<T>) {}

    fn record_budget_full(&self) {
        #[cfg(any(test, feature = "terminal-diagnostics"))]
        self.counters.command_full.fetch_add(1, Ordering::Relaxed);
    }

    fn reserve_write_bytes(&self, additional: usize) -> bool {
        self.queued_write_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(additional)
                    .filter(|&next| next <= SSH_COMMAND_BYTE_BUDGET)
            })
            .is_ok()
    }

    /// Give back budget once the task delivered (or dropped) a write.
    pub(crate) fn release_write_bytes(&self, bytes: usize) {
        self.queued_write_bytes.fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Whether close has been requested. The tokio task checks this flag to
    /// ensure it exits even if `Cmd::Close` was dropped due to a full queue.
    pub(crate) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }

    /// Latest coalesced resize, clearing the pending marker.
    pub(crate) fn take_pending_resize(&self) -> Option<(u16, u16)> {
        let mut pending = self.pending_resize.lock().unwrap();
        pending.signal_enqueued = false;
        pending.latest.take()
    }
}

impl PtyTransport for SshTransport {
    /// Write bytes to the SSH channel (via cmd_tx → tokio task → channel.data).
    /// Logs the byte count only — never the payload.
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        log::debug!("SshTransport::pty_write: {} bytes", bytes.len());
        if self.is_closing() {
            return Err(TerminalError::Closed);
        }
        if !self.reserve_write_bytes(bytes.len()) {
            self.record_budget_full();
            return Err(TerminalError::QueueFull);
        }
        match self.cmd_tx.try_send(Cmd::Write(bytes.to_vec())) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.release_write_bytes(bytes.len());
                self.record_failure(&error);
                match error {
                    TrySendError::Full(_) => Err(TerminalError::QueueFull),
                    TrySendError::Closed(_) => Err(TerminalError::Closed),
                }
            }
        }
    }

    /// Resize the SSH channel, coalescing bursts to the latest dimensions.
    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        if self.is_closing() {
            return Err(TerminalError::Closed);
        }
        let mut pending = self.pending_resize.lock().unwrap();
        pending.latest = Some((rows, cols));
        if pending.signal_enqueued {
            return Ok(());
        }
        pending.signal_enqueued = true;
        match self.cmd_tx.try_send(Cmd::Resize) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                pending.signal_enqueued = false;
                Ok(())
            }
            Err(error @ TrySendError::Closed(_)) => {
                pending.signal_enqueued = false;
                self.record_failure(&error);
                Err(TerminalError::Closed)
            }
        }
    }

    /// Close the SSH channel. Lifecycle-critical: the closing flag guarantees
    /// the task exits even if `Cmd::Close` is dropped by a full queue.
    fn pty_close(&self) -> Result<(), TerminalError> {
        self.closing.store(true, Ordering::Release);
        match self.cmd_tx.try_send(Cmd::Close) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(()),
            Err(error @ TrySendError::Closed(_)) => {
                self.record_failure(&error);
                Err(TerminalError::Closed)
            }
        }
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
