//! Typed terminal input errors and generated-input reporting.
//!
//! The public terminal capability contract remains `TerminalSession`; this module
//! intentionally contains only the error type and the small reporting helper used
//! by backend adapters whose void trait methods cannot return delivery failures.

/// Error from a terminal input/control operation (write, resize, close).
///
/// Backends return this error at the transport boundary so callers can
/// distinguish saturation, closure, and transport failures instead of silently
/// dropping input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    /// The command queue is full — the caller must retry or report failure.
    QueueFull,
    /// The session/channel is closed — no more data can be sent.
    Closed,
    /// The PTY/SSH channel encountered a transport error.
    Transport(String),
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => write!(f, "terminal command queue is full"),
            Self::Closed => write!(f, "terminal session is closed"),
            Self::Transport(msg) => write!(f, "terminal transport error: {msg}"),
        }
    }
}

impl std::error::Error for TerminalError {}

/// Log a best-effort generated input failure with operation context.
///
/// User keystrokes use the typed [`crate::TerminalSession::write`] result directly. Mouse
/// reports, clear commands, and IME commits currently have void trait methods, so
/// their delivery failures must remain observable rather than being discarded.
pub fn report_generated_input(operation: &str, result: Result<(), TerminalError>) {
    if let Err(error) = result {
        log::warn!("{operation} delivery failed: {error}");
    }
}
