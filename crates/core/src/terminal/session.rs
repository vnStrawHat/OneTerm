//! `TerminalSession` trait — render/lifecycle interface shared by the local
//! shell and SSH. The two backends implement it independently, unaware of each other.
//!
//! Pure: no GPUI dependency. Uses neutral types — the UI crate maps them to GPUI:
//! - `TerminalMouseButton` (instead of `gpui::MouseButton`).
//! - `CursorBounds` (instead of `gpui::Bounds<Pixels>`).
//! - `Receiver<SessionEvent>` from `async-channel` (instead of a GPUI channel).
//!
//! See `docs/terminal-backend.md` §9.

use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use async_channel::Receiver;

use crate::sftp::SftpBackend;
use crate::terminal::content::TerminalContent;
use crate::terminal::key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
use crate::terminal::mouse_encode::TerminalMouseButton;
use crate::terminal::osc::{Osc133Kind, TerminalProgress};
use crate::terminal::osc_color::DynamicColors;

use alacritty_terminal::vte::ansi::Rgb;

/// Basic terminal info — lightweight, does not clear damage.
/// Used for line_times updates and the scroll handle without affecting
/// damage tracking for prepaint.
#[derive(Debug, Clone, Copy)]
pub struct TerminalInfo {
    /// Total lines in scrollback + viewport (capped by scrolling_history).
    pub total_lines: usize,
    /// **Absolute** line count — total lines output since spawn, including when
    /// scrollback is full and old lines are dropped. Monotonically increasing.
    /// The gutter line number uses this value instead of `total_lines`.
    pub absolute_line_count: usize,
    /// Cursor line (alacritty Line.0).
    pub cursor_line: i32,
    /// Index (0-based, same frame as `cursor_line`) of the last line **with
    /// content** in the viewport. Used for `line_times` stamping to match the
    /// gutter region actually rendered (avoids `[--:--:--]` on lines below the cursor).
    pub last_content_line: i32,
    /// Number of visible lines (viewport height).
    pub num_lines: usize,
    /// Display offset (0 = bottom, >0 = scrolled up).
    pub display_offset: usize,
    /// Number of times the screen was cleared (`clear`/`cls`/RIS). Monotonically increasing.
    /// The UI compares it with the previous value to reset per-line timestamps (gutter):
    /// after `clear`, the absolute line counter resets → new content reuses old
    /// indices, so old timestamps must be dropped so new lines are stamped with the current time.
    pub clear_epoch: usize,
}

/// Session events emitted to the UI (subscribed via channel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// New output → the UI re-renders (debounced in the UI).
    Output,
    /// The window title changed (OSC 0/2).
    Title(String),
    /// The working directory changed (OSC 7).
    Cwd(PathBuf),
    /// Clipboard changed via OSC 52 (`None` = clear, `Some` = set).
    Clipboard(Option<String>),
    /// A program requested a clipboard read via OSC 52 (`52;c;?`). The UI should
    /// reply with the current clipboard content (see the security note: this
    /// exposes the local clipboard to programs, including remote ones over SSH).
    ClipboardRead,
    /// Shell integration marker (OSC 133) — prompt start/end, output start/end.
    ShellIntegration(Osc133Kind),
    /// Desktop notification (OSC 9) — the UI shows a toast.
    Notification(String),
    /// Taskbar progress (OSC 9;4) — the UI shows a progress indicator.
    Progress(TerminalProgress),
    /// Foreground process changed (tab title update).
    ForegroundProcess(Option<String>),
    /// Process exited (`None` = no exit code).
    Exited(Option<i32>),
    /// Session closed (PTY/SSH channel ended).
    Closed,
    /// Bell (`\x07`) — the UI shows a 🔔 indicator, cleared when the user presses a key.
    Bell,
}

/// Network statistics for a session (SSH only — local returns `None`).
/// Used by the StatusBar to display network speed.
#[derive(Debug, Clone, Copy, Default)]
pub struct NetStats {
    /// Total bytes received (download direction: server → client).
    pub rx_bytes: u64,
    /// Total bytes sent (upload direction: client → server).
    pub tx_bytes: u64,
}
/// Pixel rectangle of the cursor — for IME popup positioning.
/// The UI maps it to `gpui::Bounds<Pixels>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Common render/lifecycle interface for a terminal session.
///
/// Snapshot + input + lifecycle only — does **not** force a shared pump/transport.
/// `LocalSession` (alacritty tty + EventLoop) and `SshSession` (russh) implement
/// it independently. The two backends do not depend on each other.
pub trait TerminalSession: Send + Sync + 'static {
    // ── Render ───────────────────────────────────────────────
    /// Snapshot the grid for rendering (does not hold the lock while drawing).
    fn snapshot(&self) -> TerminalContent;

    /// Basic info (total_lines, cursor_line) — does NOT call damage()/reset_damage().
    /// Used for line_times updates without clearing damage for prepaint.
    fn terminal_info(&self) -> TerminalInfo;

    /// Alt-screen is on (e.g. vim/less) → disable IME, plain keys go through on_key_down.
    fn is_alt_screen(&self) -> bool;

    /// Dynamic OSC-set foreground/background/cursor colors (OSC 10/11/12).
    /// Read from the live `Term` color table so the renderer can apply them on
    /// top of the theme. Default = none set (use the theme).
    fn dynamic_colors(&self) -> DynamicColors {
        DynamicColors::default()
    }

    /// Provide the theme's default colors: foreground/background/cursor plus the
    /// 16-color ANSI palette. Used to answer OSC 10/11/12 and OSC 4 *queries*
    /// when the color was never set via OSC (so a bare query still reports a
    /// sensible color, e.g. for background detection). Called by the UI whenever
    /// the theme changes. Default = no-op.
    fn set_default_colors(
        &self,
        _foreground: Rgb,
        _background: Rgb,
        _cursor: Rgb,
        _ansi: [Rgb; 16],
    ) {
    }

    // ── Input ───────────────────────────────────────────────
    /// Write bytes to the PTY/channel (keystroke, paste, OSC response).
    fn write(&self, bytes: &[u8]);
    /// Flush the PTY output buffer (Windows ConPTY workaround).
    fn flush_pty(&self);
    /// Send a Ctrl+C signal — Windows uses GenerateConsoleCtrlEvent
    /// (to avoid ConPTY sending CTRL_C_EVENT to the shell), falls back to \x03.
    fn send_ctrl_c(&self);
    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16);
    /// Scroll the scrollback (only when not alt-screen / not mouse mode).
    fn scroll(&self, delta: i32);
    /// Scroll to bottom (display_offset = 0) — used when there is new output.
    fn scroll_to_bottom(&self);
    /// Scroll to top (display_offset = max) — Shift+Home.
    fn scroll_to_top(&self);

    // ── Mouse ────────────────────────────────────────────────
    /// `sel` picks the selection type when not in mouse mode: `Simple` (click),
    /// `Semantic` (double-click), `Lines` (triple-click), `Block` (alt-select).
    fn mouse_down(&self, row: f32, col: f32, button: TerminalMouseButton, sel: SelectionType);
    /// Hover (no button held) — encode mouse motion for app mode (vim/less/htop).
    /// Does NOT update the selection (only `mouse_drag` updates it).
    fn mouse_move(&self, row: f32, col: f32);
    /// Drag (left button held) — update the selection end point (non-mouse mode)
    /// or encode mouse drag (mouse mode).
    fn mouse_drag(&self, row: f32, col: f32);
    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton);
    fn wheel(&self, delta_y: f64, row: f32, col: f32);

    // ── Selection / clipboard ──────────────────────────────
    /// The currently selected text (for copy). `None` if there is no selection.
    fn selection_text(&self) -> Option<String>;
    /// Clear the current selection.
    fn clear_selection(&self);
    /// Select all content (scrollback + visible).
    fn select_all(&self);
    /// Clear screen + scrollback (send a clear escape sequence to the PTY).
    fn clear(&self);

    // ── IME ─────────────────────────────────────────────────
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
    /// Cursor position (pixels) for the IME popup.
    fn cursor_bounds(&self) -> Option<CursorBounds>;

    // ── Lifecycle ───────────────────────────────────────────
    /// Subscribe to session events (Output/Title/Cwd/Clipboard/ShellIntegration/ForegroundProcess/Exited/Closed).
    fn subscribe(&self) -> Receiver<SessionEvent>;
    /// Whether the process is still alive (not exited/closed).
    fn alive(&self) -> bool;
    /// Close the session (shut down PTY / close channel).
    fn close(&self);
    /// true = local shell, false = SSH.
    fn is_local(&self) -> bool;
    /// The current title (OSC 0/2).
    fn title(&self) -> Option<String>;
    /// The current cwd (OSC 7).
    fn cwd(&self) -> Option<PathBuf>;

    // ── Send Text / Keystroke ──────────────────────────────────
    /// Send raw text to the PTY (for automation, extensions, task runners).
    /// Equivalent to Zed `SendText(String)`.
    fn send_text(&self, text: &str) {
        self.write(text.as_bytes());
    }

    /// Send an encoded keystroke to the PTY (e.g. "Ctrl+C" → 0x03, "Enter" → \r).
    /// Equivalent to Zed `SendKeystroke(String)`.
    /// Parse format: `Ctrl+Shift+V`, `Alt+Enter`, `F1`, `Up`, `a`.
    fn send_keystroke(&self, keystroke: &str) {
        if let Some((spec, mods)) = parse_keystroke(keystroke) {
            if let Some(bytes) = encode_key(&spec, mods) {
                self.write(&bytes);
            }
        }
    }

    /// Bracketed paste mode is on → wrap the paste in `\x1b[200~...\x1b[201~`.
    /// Zed: checks `Modes::BRACKETED_PASTE` then wraps.
    fn is_bracketed_paste(&self) -> bool {
        self.snapshot().mode.contains(TermMode::BRACKETED_PASTE)
    }

    /// Paste text into the PTY. Automatically wraps it in bracketed paste markers
    /// if the terminal is in bracketed paste mode.
    fn paste(&self, text: &str) {
        if self.is_bracketed_paste() {
            let wrapped = format!("\x1b[200~{}\x1b[201~", text);
            self.write(wrapped.as_bytes());
        } else {
            self.write(text.as_bytes());
        }
    }

    // ── Shell Integration (OSC 133) ────────────────────────────
    /// Number of prompt markers captured (for scroll-to-prompt).
    /// Each marker is the line position where a prompt starts (OSC 133;A).
    fn prompt_count(&self) -> usize {
        0
    }
    /// Scroll to the `n`-th prompt (0-based, from the bottom up).
    /// `n=0` = the most recent prompt, `n=1` = the previous one, etc.
    fn scroll_to_prompt(&self, _n: usize) {}

    // ── Foreground Process ─────────────────────────────────────
    /// The current foreground process (e.g. "cargo", "node", "python").
    /// `None` = shell prompt (no command running).
    fn foreground_process(&self) -> Option<String> {
        None
    }

    // ── Breadcrumb ──────────────────────────────────────────────
    /// Text shown in the toolbar breadcrumb (e.g. the cwd path).
    fn breadcrumb_text(&self) -> Option<String> {
        self.cwd().map(|p| p.display().to_string())
    }

    // ── Network Stats ────────────────────────────────────────
    /// Network statistics (rx/tx bytes). `None` for a local shell.
    /// Used by the StatusBar to display network speed (kbps).
    fn network_stats(&self) -> Option<NetStats> {
        None
    }

    // ── SFTP ─────────────────────────────────────────────
    /// SFTP backend if the session has an SFTP channel (SSH only).
    /// `None` for a local shell — does not force local sessions to implement SFTP.
    fn sftp(&self) -> Option<Arc<dyn SftpBackend>> {
        None
    }
}

/// Parse a keystroke string → (KeySpec, KeyMods).
///
/// Format: `Ctrl+Shift+V`, `Alt+Enter`, `Up`, `F1`, `a`, `Enter`, `Tab`.
/// Modifiers are separated by `+`, case-insensitive.
///
/// Equivalent to Zed `SendKeystroke(String)`.
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
