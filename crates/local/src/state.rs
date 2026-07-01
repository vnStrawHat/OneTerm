//! Session state cache — shared (`Arc<Mutex<...>>`) between `LocalListener`
//! (updated on incoming events) and `LocalSession` (read via trait accessors).
//!
//! alacritty `Term` does not expose `title`/`cwd`/clipboard → we cache them
//! ourselves. OSC 133 (shell integration) is also cached here — prompt count +
//! last exit code.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Session state updated by the listener and read by the session.
#[derive(Debug, Default)]
pub struct SessionState {
    /// Title (OSC 0/2). `None` = reset/default.
    pub title: Option<String>,
    /// Cwd (OSC 7 — set by the side-channel parser, not an alacritty event).
    pub cwd: Option<PathBuf>,
    /// Last clipboard value via OSC 52 store.
    pub clipboard: Option<String>,
    /// Is the process still alive?
    pub alive: bool,
    /// Exit code if it has exited.
    pub exit_code: Option<i32>,
    /// Number of prompt markers (OSC 133;A) captured — for scroll-to-prompt.
    pub prompt_count: usize,
    /// Exit code of the last command (OSC 133;D;exit_code).
    pub last_exit_code: Option<i32>,
    /// Current foreground process (e.g. "cargo", "node").
    pub foreground_process: Option<String>,
    /// **Absolute** line count — total lines output even when scrollback is full.
    /// Tracked at the event loop level, read via terminal_info().
    pub absolute_line_count: usize,
    /// Previous total_lines — used by the event loop to detect dropped lines.
    pub prev_total_lines: usize,
    /// Number of times the screen was cleared (`clear`/`cls`). Bumped whenever the
    /// event loop sees `CSI 2J`/`CSI 3J`/`ESC c`. The UI uses it to reset per-line
    /// timestamps.
    pub clear_epoch: usize,
}

/// Convenient Arc-Mutex wrapper.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}
