//! The transport half a backend keeps for itself.

use crate::session::TerminalError;

/// Bytes-out side of a terminal backend: PTY or SSH channel writes, resize
/// requests, and close. Everything else (parsing, OSC routing, event delivery,
/// state caches) lives in the shared pump layer.
///
/// Implementations must be cheap to clone (Arc-shared handles): the router
/// inside `Term` and the session struct hold clones of the same transport.
/// Every method is non-blocking — `pty_write` runs from `Term` callbacks
/// (OSC/DA responses) with the `Term` lock held.
pub trait PtyTransport: Clone + Send + Sync + 'static {
    /// Queue `bytes` for delivery to the child/remote (keystrokes, paste, OSC
    /// replies). FIFO, atomic at enqueue time: `QueueFull`/`Closed` when rejected.
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError>;
    /// Request a new PTY/window size (latest value wins).
    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError>;
    /// Ask the owner loop to shut the transport down (out of band, never dropped).
    fn pty_close(&self) -> Result<(), TerminalError>;
}
