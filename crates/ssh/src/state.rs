//! Trạng thái session cache — shared (`Arc<Mutex<...>>`) giữa `SshListener`
//! (cập nhật khi nhận event) và `SshSession` (đọc qua trait accessors).
//!
//! Tương tự `local/src/state.rs` — alacritty `Term` không expose `title`/`cwd`/
//! clipboard → phải tự cache.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Trạng thái session được listener cập nhật + session đọc.
#[derive(Debug, Default)]
pub struct SessionState {
    /// Tiêu đề (OSC 0/2). `None` = reset/default.
    pub title: Option<String>,
    /// Cwd (OSC 7 — set bởi side-channel parser).
    pub cwd: Option<PathBuf>,
    /// Clipboard cuối qua OSC 52 store.
    pub clipboard: Option<String>,
    /// Process còn sống?
    pub alive: bool,
    /// Exit code nếu đã thoát.
    pub exit_code: Option<i32>,
    /// Số prompt markers (OSC 133;A).
    pub prompt_count: usize,
    /// Exit code của command cuối (OSC 133;D;exit_code).
    pub last_exit_code: Option<i32>,
    /// Foreground process hiện tại.
    pub foreground_process: Option<String>,
    /// Absolute line count (xem local crate).
    pub absolute_line_count: usize,
    /// Previous total_lines — detect dropped lines.
    pub prev_total_lines: usize,
    /// Số lần màn hình bị xoá (`clear`). Xem `local/src/state.rs`.
    pub clear_epoch: usize,
    /// Tổng bytes nhận từ server (download direction).
    pub rx_bytes: u64,
    /// Tổng bytes gửi lên server (upload direction).
    pub tx_bytes: u64,
}

/// Arc-Mutex wrapper.
pub type SharedState = Arc<Mutex<SessionState>>;

pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(SessionState::default()))
}
