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
use alacritty_terminal::term::TermMode;
use async_channel::Receiver;

use crate::terminal::content::TerminalContent;
use crate::terminal::key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
use crate::terminal::mouse_encode::TerminalMouseButton;
use crate::terminal::osc::Osc133Kind;

/// Basic terminal info — lightweight, không clear damage.
/// Dùng cho line_times update và scroll handle mà không ảnh hưởng
/// damage tracking cho prepaint.
#[derive(Debug, Clone, Copy)]
pub struct TerminalInfo {
    /// Tổng số dòng trong scrollback + viewport.
    pub total_lines: usize,
    /// Cursor line (alacritty Line.0).
    pub cursor_line: i32,
    /// Số dòng hiển thị (viewport height).
    pub num_lines: usize,
    /// Display offset (0 = bottom, >0 = scrolled up).
    pub display_offset: usize,
}

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
    /// Shell integration marker (OSC 133) — prompt start/end, output start/end.
    ShellIntegration(Osc133Kind),
    /// Foreground process đổi (tab title update).
    ForegroundProcess(Option<String>),
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

    /// Basic info (total_lines, cursor_line) — KHÔNG gọi damage()/reset_damage().
    /// Dùng cho line_times update mà không clear damage cho prepaint.
    fn terminal_info(&self) -> TerminalInfo;

    /// Alt-screen đang bật (vd vim/less) → tắt IME, phím thường qua on_key_down.
    fn is_alt_screen(&self) -> bool;

    // ── Input ───────────────────────────────────────────────
    /// Ghi byte vào PTY/channel (keystroke, paste, OSC response).
    fn write(&self, bytes: &[u8]);
    /// Flush PTY output buffer (Windows ConPTY workaround).
    fn flush_pty(&self);
    /// Gửi Ctrl+C signal — Windows dùng GenerateConsoleCtrlEvent
    /// (tránh ConPTY gửi CTRL_C_EVENT đến shell), fallback \x03.
    fn send_ctrl_c(&self);
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
    /// Subscribe sự kiện session (Output/Title/Cwd/Clipboard/ShellIntegration/ForegroundProcess/Exited/Closed).
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

    // ── Send Text / Keystroke ──────────────────────────────────
    /// Gửi raw text vào PTY (cho automation, extension, task runner).
    /// Tương đương Zed `SendText(String)`.
    fn send_text(&self, text: &str) {
        self.write(text.as_bytes());
    }

    /// Gửi keystroke encoded vào PTY (vd "Ctrl+C" → 0x03, "Enter" → \r).
    /// Tương đương Zed `SendKeystroke(String)`.
    /// Parse format: `Ctrl+Shift+V`, `Alt+Enter`, `F1`, `Up`, `a`.
    fn send_keystroke(&self, keystroke: &str) {
        if let Some((spec, mods)) = parse_keystroke(keystroke) {
            if let Some(bytes) = encode_key(&spec, mods) {
                self.write(&bytes);
            }
        }
    }

    /// Bracketed paste mode đang bật → wrap paste trong `\x1b[200~...\x1b[201~`.
    /// Zed: kiểm `Modes::BRACKETED_PASTE` rồi wrap.
    fn is_bracketed_paste(&self) -> bool {
        self.snapshot().mode.contains(TermMode::BRACKETED_PASTE)
    }

    /// Paste text vào PTY. Tự động wrap trong bracketed paste markers nếu
    /// terminal đang ở bracketed paste mode.
    fn paste(&self, text: &str) {
        if self.is_bracketed_paste() {
            let wrapped = format!("\x1b[200~{}\x1b[201~", text);
            self.write(wrapped.as_bytes());
        } else {
            self.write(text.as_bytes());
        }
    }

    // ── Shell Integration (OSC 133) ────────────────────────────
    /// Số dòng prompt markers đã capture (cho scroll-to-prompt).
    /// Mỗi marker là vị trí dòng nơi prompt bắt đầu (OSC 133;A).
    fn prompt_count(&self) -> usize {
        0
    }
    /// Scroll đến prompt thứ `n` (0-based, từ cuối lên).
    /// `n=0` = prompt gần nhất, `n=1` = prompt trước đó, v.v.
    fn scroll_to_prompt(&self, _n: usize) {}

    // ── Foreground Process ─────────────────────────────────────
    /// Foreground process hiện tại (vd "cargo", "node", "python").
    /// `None` = shell prompt (không có command chạy).
    fn foreground_process(&self) -> Option<String> {
        None
    }

    // ── Breadcrumb ──────────────────────────────────────────────
    /// Text hiển thị trong toolbar breadcrumb (vd cwd path).
    fn breadcrumb_text(&self) -> Option<String> {
        self.cwd().map(|p| p.display().to_string())
    }
}

/// Parse keystroke string → (KeySpec, KeyMods).
///
/// Format: `Ctrl+Shift+V`, `Alt+Enter`, `Up`, `F1`, `a`, `Enter`, `Tab`.
/// Modifiers cách nhau bằng `+`, case-insensitive.
///
/// Tương đương Zed `SendKeystroke(String)`.
pub fn parse_keystroke(s: &str) -> Option<(KeySpec, KeyMods)> {
    let parts: Vec<&str> = s.split('+').collect();
    let mut mods = KeyMods::default();
    let mut key_part = s;

    // Parse modifiers (all parts except last).
    if parts.len() > 1 {
        for part in &parts[..parts.len() - 1] {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" | "option" | "opt" => mods.alt = true,
                _ => return None, // Unknown modifier
            }
        }
        key_part = parts[parts.len() - 1];
    }

    let named = match key_part.to_lowercase().as_str() {
        "enter" | "return" => Some(NamedKey::Enter),
        "backspace" | "bs" => Some(NamedKey::Backspace),
        "delete" | "del" => Some(NamedKey::Delete),
        "tab" => Some(NamedKey::Tab),
        "escape" | "esc" => Some(NamedKey::Escape),
        "up" | "arrowup" => Some(NamedKey::ArrowUp),
        "down" | "arrowdown" => Some(NamedKey::ArrowDown),
        "left" | "arrowleft" => Some(NamedKey::ArrowLeft),
        "right" | "arrowright" => Some(NamedKey::ArrowRight),
        "home" => Some(NamedKey::Home),
        "end" => Some(NamedKey::End),
        "pageup" | "pgup" => Some(NamedKey::PageUp),
        "pagedown" | "pgdn" => Some(NamedKey::PageDown),
        "insert" | "ins" => Some(NamedKey::Insert),
        _ => None,
    };

    let spec = if let Some(n) = named {
        KeySpec::Named(n)
    } else {
        // Single character key.
        if key_part.is_empty() || key_part.chars().count() > 1 {
            return None;
        }
        KeySpec::Character(key_part.to_string())
    };

    Some((spec, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_c() {
        let (spec, mods) = parse_keystroke("Ctrl+C").unwrap();
        assert!(matches!(spec, KeySpec::Character(c) if c == "C"));
        assert!(mods.ctrl);
        assert!(!mods.shift);
        assert!(!mods.alt);
    }

    #[test]
    fn parse_ctrl_shift_v() {
        let (spec, mods) = parse_keystroke("Ctrl+Shift+V").unwrap();
        assert!(matches!(spec, KeySpec::Character(c) if c == "V"));
        assert!(mods.ctrl);
        assert!(mods.shift);
    }

    #[test]
    fn parse_enter() {
        let (spec, mods) = parse_keystroke("Enter").unwrap();
        assert!(matches!(spec, KeySpec::Named(NamedKey::Enter)));
        assert!(!mods.ctrl);
    }

    #[test]
    fn parse_alt_enter() {
        let (spec, mods) = parse_keystroke("Alt+Enter").unwrap();
        assert!(matches!(spec, KeySpec::Named(NamedKey::Enter)));
        assert!(mods.alt);
    }

    #[test]
    fn parse_arrow_up() {
        let (spec, _) = parse_keystroke("Up").unwrap();
        assert!(matches!(spec, KeySpec::Named(NamedKey::ArrowUp)));
    }

    #[test]
    fn parse_unknown_modifier() {
        assert!(parse_keystroke("Foo+A").is_none());
    }

    #[test]
    fn parse_empty() {
        assert!(parse_keystroke("").is_none());
    }
}
