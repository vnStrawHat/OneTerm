//! Session state cache — shared (`Arc<Mutex<...>>`) between `LocalListener`
//! (updated on incoming events) and `LocalSession` (read via trait accessors).
//!
//! alacritty `Term` does not expose `title`/`cwd`/clipboard → we cache them
//! ourselves. OSC 133 (shell integration) is also cached here — prompt count +
//! last exit code.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::vte::ansi::Rgb;

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
    /// Theme default foreground — used to answer OSC 10 queries when the program
    /// never set it via OSC. Set by the UI via `set_default_colors`.
    pub default_foreground: Option<Rgb>,
    /// Theme default background — used to answer OSC 11 queries.
    pub default_background: Option<Rgb>,
    /// Theme default cursor color — used to answer OSC 12 queries.
    pub default_cursor: Option<Rgb>,
    /// Theme default 16-color ANSI palette — used to answer OSC 4 queries for
    /// indices 0-15 that were never set via OSC.
    pub default_ansi: Option<[Rgb; 16]>,
    /// Last applied `seq` per agent id (OSC 9;7 dedup, spec §4.1 / §8.3).
    /// Events with `seq <= last_applied_seq` for the same agent are dropped.
    pub last_agent_seq: std::collections::HashMap<String, u64>,
}

/// Convenient Arc-Mutex wrapper.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}
