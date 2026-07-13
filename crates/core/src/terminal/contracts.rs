//! Typed errors and sub-trait contracts for terminal sessions.
//!
//! The current `TerminalSession` trait is a monolithic aggregate spanning
//! rendering, input, lifecycle, search, IME, SFTP, and state. Phase 2
//! introduces typed errors and documents the intended sub-trait split.
//! The full migration (moving all consumers to the sub-traits) is Phase 5
//! work — done after the shared local/SSH adapter is extracted, so the
//! split does not preserve the current duplicated API surface.

use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::CursorShape;

use crate::terminal::content::TerminalContent;
use crate::terminal::mouse_encode::MouseModifiers;
use crate::terminal::osc_color::DynamicColors;

use super::TerminalQueryState;

/// Error from a terminal input/control operation (write, resize, close).
///
/// Currently both backends use `try_send` into bounded channels and only
/// log failures. The typed error lets future callers surface transport
/// failures to the UI instead of silently dropping input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    /// The command queue is full — input was dropped.
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

// ── Sub-trait contracts (documentation of the intended split) ──────────────
//
// These traits are not yet used by consumers — they document the target
// architecture. Phase 5 will migrate callers to use these narrower
// interfaces instead of the monolithic `TerminalSession`.

/// Render-only interface: produce frames and compact query state.
///
/// The UI renderer calls `snapshot()` exactly once per prepaint (consumes
/// damage) and `query_state()` for lightweight mode/cursor/size reads.
pub trait TerminalRenderer: Send + Sync + 'static {
    /// Snapshot the grid for rendering — **consumes and resets damage**.
    /// Call only from the render/prepaint path, exactly once per frame.
    fn snapshot(&self) -> TerminalContent;

    /// Compact query state — mode, cursor, viewport size. O(1), no cell clone.
    fn query_state(&self) -> TerminalQueryState;

    /// Dynamic OSC-set foreground/background/cursor colors.
    fn dynamic_colors(&self) -> DynamicColors {
        DynamicColors::default()
    }

    /// Provide the theme's default colors for OSC color queries.
    fn set_default_colors(
        &self,
        _foreground: alacritty_terminal::vte::ansi::Rgb,
        _background: alacritty_terminal::vte::ansi::Rgb,
        _cursor: alacritty_terminal::vte::ansi::Rgb,
        _ansi: [alacritty_terminal::vte::ansi::Rgb; 16],
    ) {
    }
}

/// Input interface: ordered writes, paste, mouse, resize — with typed errors.
///
/// Writes are ordered (FIFO) and must not be silently dropped. Close is
/// lifecycle-critical and must always be honored. Resize is coalescible
/// (latest wins).
pub trait TerminalInput: Send + Sync + 'static {
    /// Write bytes to the PTY/channel. Returns an error if the transport
    /// is closed or the queue is saturated.
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError>;

    /// Flush the PTY output buffer (Windows ConPTY workaround).
    fn flush_pty(&self);

    /// Send Ctrl+C signal.
    fn send_ctrl_c(&self);

    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError>;

    /// Close the session — lifecycle-critical, must always be honored.
    fn close(&self) -> Result<(), TerminalError>;
}

/// Lifecycle interface: events, alive state, cancellation.
pub trait TerminalLifecycle: Send + Sync + 'static {
    type Event;

    /// Subscribe to session events.
    fn subscribe(&self) -> async_channel::Receiver<Self::Event>;

    /// Whether the process is still alive.
    fn alive(&self) -> bool;

    /// true = local shell, false = SSH.
    fn is_local(&self) -> bool;

    /// The current title (OSC 0/2).
    fn title(&self) -> Option<String>;
}

/// Capability flag: whether the terminal is in a mode that affects input.
pub trait TerminalModeQuery: Send + Sync + 'static {
    /// Alt-screen is on (vim/less) → disable IME.
    fn is_alt_screen(&self) -> bool;

    /// Current mode bits.
    fn mode(&self) -> TermMode;

    /// Cursor shape for IME/rendering decisions.
    fn cursor_shape(&self) -> CursorShape;
}

/// Mouse input interface (part of TerminalInput, separated for clarity).
pub trait TerminalMouse: Send + Sync + 'static {
    fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: crate::terminal::mouse_encode::TerminalMouseButton,
        sel: alacritty_terminal::selection::SelectionType,
        mods: MouseModifiers,
    );
    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers);
    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers);
    fn mouse_up(
        &self,
        row: f32,
        col: f32,
        button: crate::terminal::mouse_encode::TerminalMouseButton,
        mods: MouseModifiers,
    );
    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers);
}
