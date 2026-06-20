//! `TerminalSession` trait — render/lifecycle interface dùng chung cho local
//! shell và SSH. Hai backend implement độc lập, không biết nhau.
//!
//! Thuần: không phụ thuộc GPUI. Dùng type trung tính — UI crate map sang GPUI:
//! - `TerminalMouseButton` (thay `gpui::MouseButton`).
//! - `CursorBounds` (thay `gpui::Bounds<Pixels>`).
//! - `Receiver<SessionEvent>` từ `async-channel` (thay GPUI channel).
//!
//! Tham chiếu `docs/terminal-backend.md` §9.

use std::path::PathBuf;

use alacritty_terminal::selection::SelectionType;
use async_channel::Receiver;

use crate::terminal::content::TerminalContent;
use crate::terminal::mouse_encode::TerminalMouseButton;

/// Sự kiện session phát ra cho UI (subscribe qua channel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// Có output mới → UI re-render (debounce ở UI).
    Output,
    /// Tiêu đề cửa sổ đổi (OSC 0/2).
    Title(String),
    /// Thư mục làm việc đổi (OSC 7).
    Cwd(PathBuf),
    /// Clipboard đổi qua OSC 52 (`None` = clear, `Some` = set).
    Clipboard(Option<String>),
    /// Process thoát (`None` = không có exit code).
    Exited(Option<i32>),
    /// Session đóng (PTY/SSH channel kết thúc).
    Closed,
    /// Bell (`\x07`) — UI show 🔔 indicator, clear khi user gõ phím.
    Bell,
}

/// Hình chữ nhật pixel của con trỏ — cho IME popup positioning.
/// UI map sang `gpui::Bounds<Pixels>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Render/lifecycle interface chung cho terminal session.
///
/// Chỉ snapshot + input + lifecycle — **không** ép pump/transport chung.
/// `LocalSession` (alacritty tty + EventLoop) và `SshSession` (russh) implement
/// độc lập. Hai backend không phụ thuộc lẫn nhau.
pub trait TerminalSession: Send + Sync + 'static {
    // ── Render ───────────────────────────────────────────────
    /// Snapshot grid để render (không giữ lock khi vẽ).
    fn snapshot(&self) -> TerminalContent;

    /// Alt-screen đang bật (vd vim/less) → tắt IME, phím thường qua on_key_down.
    fn is_alt_screen(&self) -> bool;

    // ── Input ───────────────────────────────────────────────
    /// Ghi byte vào PTY/channel (keystroke, paste, OSC response).
    fn write(&self, bytes: &[u8]);
    /// Flush PTY output buffer (Windows ConPTY workaround).
    fn flush_pty(&self);
    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16);
    /// Scroll scrollback (chỉ khi không alt-screen / không mouse mode).
    fn scroll(&self, delta: i32);
    /// Scroll to bottom (display_offset = 0) — dùng khi có output mới.
    fn scroll_to_bottom(&self);
    /// Scroll to top (display_offset = max) — Shift+Home.
    fn scroll_to_top(&self);

    // ── Mouse ────────────────────────────────────────────────
    /// `sel` chọn loại selection khi không ở mouse mode: `Simple` (click),
    /// `Semantic` (double-click), `Lines` (triple-click), `Block` (alt-select).
    fn mouse_down(&self, row: f32, col: f32, button: TerminalMouseButton, sel: SelectionType);
    /// Hover (no button held) — encode mouse motion cho app mode (vim/less/htop).
    /// KHÔNG cập nhật selection (chỉ `mouse_drag` mới cập nhật).
    fn mouse_move(&self, row: f32, col: f32);
    /// Drag (left button held) — cập nhật selection end point (non-mouse mode)
    /// hoặc encode mouse drag (mouse mode).
    fn mouse_drag(&self, row: f32, col: f32);
    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton);
    fn wheel(&self, delta_y: f64, row: f32, col: f32);

    // ── Selection / clipboard ──────────────────────────────
    /// Text đang được chọn (cho copy). `None` nếu không selection.
    fn selection_text(&self) -> Option<String>;
    /// Xóa selection hiện tại.
    fn clear_selection(&self);
    /// Select toàn bộ nội dung (scrollback + visible).
    fn select_all(&self);
    /// Clear screen + scrollback (gửi escape sequence clear tới PTY).
    fn clear(&self);

    // ── IME ─────────────────────────────────────────────────
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
    /// Vị trí con trỏ (pixel) cho IME popup.
    fn cursor_bounds(&self) -> Option<CursorBounds>;

    // ── Lifecycle ───────────────────────────────────────────
    /// Subscribe sự kiện session (Output/Title/Cwd/Clipboard/Exited/Closed).
    fn subscribe(&self) -> Receiver<SessionEvent>;
    /// Process còn sống (chưa exit/close).
    fn alive(&self) -> bool;
    /// Đóng session (shutdown PTY / close channel).
    fn close(&self);
    /// true = local shell, false = SSH.
    fn is_local(&self) -> bool;
    /// Tiêu đề hiện tại (OSC 0/2).
    fn title(&self) -> Option<String>;
    /// Cwd hiện tại (OSC 7).
    fn cwd(&self) -> Option<PathBuf>;
}
