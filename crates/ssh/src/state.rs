//! Session state cache — shared (`Arc<Mutex<...>>`) between `SshListener`
//! (updated on incoming events) and `SshSession` (read via trait accessors).
//!
//! Similar to `local/src/state.rs` — alacritty `Term` does not expose
//! `title`/`cwd`/clipboard → we cache them ourselves.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::vte::ansi::Rgb;

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

/// Arc-Mutex wrapper.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}

/// Live cwd reader shared with the UI — reads `SessionState.cwd` (OSC 7) on demand.
///
/// Clones cheaply (`Arc`) and points at the same state the listener updates when
/// it parses OSC 7, so reads always reflect the latest `cd`. Used by the SFTP
/// browser's "sync to terminal cwd" button. See `docs/sftp-follow-terminal-cwd.md`.
pub struct SshCwdSource {
    state: SharedState,
}

impl SshCwdSource {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl oneterm_terminal::CwdSource for SshCwdSource {
    fn cwd(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().cwd.clone()
    }
}
