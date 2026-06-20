//! Trạng thái session cache — shared (`Arc<Mutex<...>>`) giữa `LocalListener`
//! (cập nhật khi nhận event) và `LocalSession` (đọc qua trait accessors).
//!
//! alacritty `Term` không expose `title`/`cwd`/clipboard → phải tự cache.
//! OSC 133 (shell integration) cũng cache ở đây — prompt count + last exit code.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Trạng thái session được listener cập nhật + session đọc.
#[derive(Debug, Default)]
pub struct SessionState {
    /// Tiêu đề (OSC 0/2). `None` = reset/default.
    pub title: Option<String>,
    /// Cwd (OSC 7 — set bởi side-channel parser, không phải alacritty event).
    pub cwd: Option<PathBuf>,
    /// Clipboard cuối qua OSC 52 store.
    pub clipboard: Option<String>,
    /// Process còn sống?
    pub alive: bool,
    /// Exit code nếu đã thoát.
    pub exit_code: Option<i32>,
    /// Số prompt markers (OSC 133;A) đã capture — cho scroll-to-prompt.
    pub prompt_count: usize,
    /// Exit code của command cuối (OSC 133;D;exit_code).
    pub last_exit_code: Option<i32>,
    /// Foreground process hiện tại (vd "cargo", "node").
    pub foreground_process: Option<String>,
}

/// Arc-Mutex wrapper tiện lợi.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}
