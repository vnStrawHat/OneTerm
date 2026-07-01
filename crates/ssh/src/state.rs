//! Session state cache — shared (`Arc<Mutex<...>>`) between `SshListener`
//! (updated on incoming events) and `SshSession` (read via trait accessors).
//!
//! Similar to `local/src/state.rs` — alacritty `Term` does not expose
//! `title`/`cwd`/clipboard → we cache them ourselves.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Session state updated by the listener and read by the session.
#[derive(Debug, Default)]
pub struct SessionState {
    /// Title (OSC 0/2). `None` = reset/default.
    pub title: Option<String>,
    /// Cwd (OSC 7 — set by the side-channel parser).
    pub cwd: Option<PathBuf>,
    /// Last clipboard value via OSC 52 store.
    pub clipboard: Option<String>,
    /// Is the process still alive?
    pub alive: bool,
    /// Exit code if it has exited.
    pub exit_code: Option<i32>,
    /// Number of prompt markers (OSC 133;A).
    pub prompt_count: usize,
    /// Exit code of the last command (OSC 133;D;exit_code).
    pub last_exit_code: Option<i32>,
    /// Current foreground process.
    pub foreground_process: Option<String>,
    /// Absolute line count (see the local crate).
    pub absolute_line_count: usize,
    /// Previous total_lines — to detect dropped lines.
    pub prev_total_lines: usize,
    /// Number of times the screen was cleared (`clear`). See `local/src/state.rs`.
    pub clear_epoch: usize,
    /// Total bytes received from the server (download direction).
    pub rx_bytes: u64,
    /// Total bytes sent to the server (upload direction).
    pub tx_bytes: u64,
}

/// Arc-Mutex wrapper.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}
