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
use alacritty_terminal::vte::ansi::Rgb;
use async_channel::Receiver;
use oneterm_core::sftp::SftpBackend;

use crate::IndexedCell;
use crate::content::TerminalContent;
use crate::contracts::TerminalError;
use crate::key_encode::{KeyMods, KeySpec, NamedKey, encode_key};
use crate::mouse_encode::{MouseModifiers, TerminalMouseButton};
use crate::osc::{Osc133Kind, TerminalProgress};
use crate::osc_agent::AgentStatusEvent;
use crate::osc_color::DynamicColors;
use crate::paste::{PasteMode, PastePolicy, PasteResult, encode_paste};
use crate::search::{SearchMatch, SearchOptions};

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
    /// Number of columns (viewport width).
    pub num_cols: usize,
    /// Display offset (0 = bottom, >0 = scrolled up).
    pub display_offset: usize,
    /// Number of times the screen was cleared (`clear`/`cls`/RIS). Monotonically increasing.
    /// The UI compares it with the previous value to reset per-line timestamps (gutter):
    /// after `clear`, the absolute line counter resets → new content reuses old
    /// indices, so old timestamps must be dropped so new lines are stamped with the current time.
    pub clear_epoch: usize,
}

/// Compact query state for non-render reads — does NOT clone the full grid.
///
/// Used for mode checks, cursor positioning, viewport size, and scroll info
/// without the O(rows×cols) cost of [`TerminalContent`].
#[derive(Debug, Clone, Copy)]
pub struct TerminalQueryState {
    /// Terminal mode (mouse, alt-screen, bracketed paste, app cursor…).
    pub mode: TermMode,
    /// Cursor display position (line.0 = top of viewport, column.0 = left).
    pub cursor_line: i32,
    pub cursor_col: usize,
    /// Cursor shape (Hidden, Block, Beam, Underline).
    pub cursor_shape: alacritty_terminal::vte::ansi::CursorShape,
    /// Display offset (0 = at bottom, >0 = scrolled up).
    pub display_offset: usize,
    /// Viewport dimensions.
    pub rows: usize,
    pub cols: usize,
    /// Total lines (scrollback + viewport).
    pub total_lines: usize,
    /// Whether the process is alive.
    pub alive: bool,
}
/// Session events emitted to the UI (subscribed via channel).
///
/// [`SessionEvent::Output`] is a coalescible repaint hint. Every other variant
/// is reliable and applies bounded-channel backpressure instead of being dropped.
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
    /// Coding-agent status event (OSC 9;7, see `docs/osc-agent-status.md`).
    /// Wrapped in `Arc` so cloning on the fan-out path is cheap. `seq` dedup
    /// is already applied by the listener before forwarding.
    AgentStatus(std::sync::Arc<AgentStatusEvent>),
    /// Foreground process changed (tab title update).
    ForegroundProcess(Option<String>),
    /// Process exited (`None` = no exit code).
    Exited(Option<i32>),
    /// Session closed (PTY/SSH channel ended).
    Closed,
    /// Bell (`\x07`) — the UI shows a 🔔 indicator, cleared when the user presses a key.
    Bell,
}

impl SessionEvent {
    /// Return the delivery policy required by this event.
    pub const fn delivery_policy(&self) -> SessionEventDelivery {
        match self {
            Self::Output => SessionEventDelivery::Coalescible,
            Self::Title(_)
            | Self::Cwd(_)
            | Self::Clipboard(_)
            | Self::ClipboardRead
            | Self::ShellIntegration(_)
            | Self::Notification(_)
            | Self::Progress(_)
            | Self::AgentStatus(_)
            | Self::ForegroundProcess(_)
            | Self::Exited(_)
            | Self::Closed
            | Self::Bell => SessionEventDelivery::Reliable,
        }
    }
}

/// Delivery policy for session events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEventDelivery {
    /// The event is a repaint hint and may be coalesced under load.
    Coalescible,
    /// The event must be delivered or report that the channel is closed.
    Reliable,
}

/// Live source of a session's current working directory (OSC 7).
///
/// Lets the UI read the cwd on demand without holding a reference to the session
/// entity or importing the `ssh`/`local` crates. Backends provide an `Arc<dyn CwdSource>`
/// that shares the same state the OSC 7 parser updates, so reads are always live.
///
/// Used by the SFTP browser's "sync to terminal cwd" button.
pub trait CwdSource: Send + Sync {
    /// The current working directory (OSC 7). `None` if no OSC 7 has been received.
    fn cwd(&self) -> Option<PathBuf>;
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

/// Optional session capabilities that are not required by every backend.
///
/// The stable [`TerminalSession`] façade exposes this single capability object
/// so adding another optional backend feature does not require a new default
/// method on every session implementation and test fake.
#[derive(Clone, Default)]
pub struct TerminalCapabilities {
    /// SSH network counters, if the backend exposes them.
    pub network_stats: Option<NetStats>,
    /// SFTP access, if the backend exposes a remote filesystem channel.
    pub sftp: Option<Arc<dyn SftpBackend>>,
    /// Live OSC 7 working-directory source, if available.
    pub cwd_source: Option<Arc<dyn CwdSource>>,
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
    ///
    /// **Consumes and resets** the terminal damage — call this **only** from the
    /// render/prepaint path, exactly once per frame. For any other read
    /// (cursor bounds, mouse hit-test, URL detection, mode checks) use
    /// [`snapshot_query`](Self::snapshot_query), which does not touch damage.
    fn snapshot(&self) -> TerminalContent;

    /// Snapshot for auxiliary (non-render) reads — does **not** consume/reset the
    /// terminal damage, so it cannot starve the renderer of dirty-row info and
    /// leave stale rows on screen.
    ///
    /// Real backends override this with a damage-free snapshot.
    fn snapshot_query(&self) -> TerminalContent;

    /// Compact query state for non-render reads — mode, cursor, viewport size.
    /// Does NOT clone the full grid (O(1) vs O(rows×cols) for `snapshot_query`).
    /// Use this for mode checks, cursor positioning, and viewport-size reads.
    ///
    /// Default falls back to `snapshot_query()` for compatibility; real backends
    /// override it with a damage-free, cell-free query.
    fn query_state(&self) -> TerminalQueryState {
        let snap = self.snapshot_query();
        TerminalQueryState {
            mode: snap.mode,
            cursor_line: snap.cursor.point.line.0,
            cursor_col: snap.cursor.point.column.0,
            cursor_shape: snap.cursor.shape,
            display_offset: snap.display_offset,
            rows: snap.terminal_bounds.num_lines,
            cols: snap.terminal_bounds.num_cols,
            total_lines: snap.total_lines,
            alive: self.alive(),
        }
    }

    /// Read cells for a range of display lines (0-based from top of viewport).
    /// Cheaper than `snapshot_query` (O(window×cols) vs O(rows×cols)) — used for
    /// URL hover detection where only the lines near the cursor are needed.
    ///
    /// Returns `(cells, num_cols)` where `cells` has up to `count × num_cols` entries,
    /// starting from `start_line`. Default falls back to `snapshot_query` for compatibility.
    fn query_line_range_cells(&self, start_line: usize, count: usize) -> (Vec<IndexedCell>, usize) {
        let snap = self.snapshot_query();
        let num_cols = snap.terminal_bounds.num_cols;
        let start = start_line * num_cols;
        let end = (start + count * num_cols).min(snap.cells.len());
        let cells = if start <= snap.cells.len() {
            snap.cells[start..end].to_vec()
        } else {
            Vec::new()
        };
        (cells, num_cols)
    }

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
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError>;
    /// Flush the PTY output buffer (Windows ConPTY workaround).
    fn flush_pty(&self);
    /// Send a Ctrl+C signal — Windows uses GenerateConsoleCtrlEvent
    /// (to avoid ConPTY sending CTRL_C_EVENT to the shell), falls back to \x03.
    fn send_ctrl_c(&self);
    /// Resize rows×cols (PTY resize / ssh window_change).
    fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError>;
    /// Scroll the scrollback (only when not alt-screen / not mouse mode).
    fn scroll(&self, delta: i32);
    /// Scroll to bottom (display_offset = 0) — used when there is new output.
    fn scroll_to_bottom(&self);
    /// Scroll to top (display_offset = max) — Shift+Home.
    fn scroll_to_top(&self);

    // ── Mouse ────────────────────────────────────────────────
    /// `sel` picks the selection type when not in mouse mode: `Simple` (click),
    /// `Semantic` (double-click), `Lines` (triple-click), `Block` (alt-select).
    fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        sel: SelectionType,
        mods: MouseModifiers,
    );
    /// Hover (no button held) — encode mouse motion for app mode (vim/less/htop).
    /// Does NOT update the selection (only `mouse_drag` updates it).
    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers);
    /// Drag (left button held) — update the selection end point (non-mouse mode)
    /// or encode mouse drag (mouse mode).
    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers);
    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton, mods: MouseModifiers);
    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers);

    // ── Selection / clipboard ──────────────────────────────
    /// The currently selected text (for copy). `None` if there is no selection.
    fn selection_text(&self) -> Option<String>;
    /// Clear the current selection.
    fn clear_selection(&self);
    /// Select all content (scrollback + visible).
    fn select_all(&self);
    /// Clear screen + scrollback (send a clear escape sequence to the PTY).
    fn clear(&self);

    // ── Search ──────────────────────────────────────────────
    /// Search the full scrollback + viewport for `query` and return matches in
    /// grid coordinates (top-to-bottom order). Empty query → empty result.
    ///
    /// Default = no matches (sessions that don't implement search just return
    /// an empty vec). Backends lock their `Term` and call
    /// [`crate::search::search_term`].
    fn search(&self, _query: &str, _options: SearchOptions) -> Vec<SearchMatch> {
        Vec::new()
    }

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
    fn close(&self) -> Result<(), TerminalError>;
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
        if let Err(error) = self.write(text.as_bytes()) {
            log::warn!("terminal send_text failed: {error}");
        }
    }

    /// Send an encoded keystroke to the PTY (e.g. "Ctrl+C" → 0x03, "Enter" → \r).
    /// Equivalent to Zed `SendKeystroke(String)`.
    /// Parse format: `Ctrl+Shift+V`, `Alt+Enter`, `F1`, `Up`, `a`.
    fn send_keystroke(&self, keystroke: &str) {
        if let Some((spec, mods)) = parse_keystroke(keystroke) {
            let app_cursor = self.query_state().mode.contains(TermMode::APP_CURSOR);
            if let Some(bytes) = encode_key(&spec, mods, app_cursor) {
                if let Err(error) = self.write(&bytes) {
                    log::warn!("terminal send_keystroke failed: {error}");
                }
            }
        }
    }

    /// Bracketed paste mode is on → wrap the paste in `\x1b[200~...\x1b[201~`.
    /// Zed: checks `Modes::BRACKETED_PASTE` then wraps.
    fn is_bracketed_paste(&self) -> bool {
        self.query_state().mode.contains(TermMode::BRACKETED_PASTE)
    }

    /// Paste text into the PTY. Automatically wraps it in bracketed paste markers
    /// if the terminal is in bracketed paste mode.
    fn paste(&self, text: &str) {
        let mode = if self.is_bracketed_paste() {
            PasteMode::Bracketed
        } else {
            PasteMode::Plain
        };
        let policy = PastePolicy::default();
        let result = match encode_paste(text, mode, &policy) {
            PasteResult::Ok(bytes) => self.write(&bytes),
            PasteResult::TooLarge(_) => {
                log::warn!("paste rejected: exceeded max paste size");
                return;
            }
        };
        if let Err(error) = result {
            log::warn!("terminal paste failed: {error}");
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

    // ── Optional capabilities ───────────────────────────────
    /// Return capabilities provided by this backend without widening the stable
    /// session façade for every future optional feature.
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities::default()
    }
}

/// Parse a keystroke string → (KeySpec, KeyMods).
///
/// Format: `Ctrl+Shift+V`, `Alt+Enter`, `Up`, `F1`, `a`, `Enter`, `Tab`.
/// Modifiers are separated by `+`, case-insensitive.
///
/// Equivalent to Zed `SendKeystroke(String)`.
pub(crate) fn parse_keystroke(s: &str) -> Option<(KeySpec, KeyMods)> {
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
        "f1" => Some(NamedKey::F1),
        "f2" => Some(NamedKey::F2),
        "f3" => Some(NamedKey::F3),
        "f4" => Some(NamedKey::F4),
        "f5" => Some(NamedKey::F5),
        "f6" => Some(NamedKey::F6),
        "f7" => Some(NamedKey::F7),
        "f8" => Some(NamedKey::F8),
        "f9" => Some(NamedKey::F9),
        "f10" => Some(NamedKey::F10),
        "f11" => Some(NamedKey::F11),
        "f12" => Some(NamedKey::F12),
        "f13" => Some(NamedKey::F13),
        "f14" => Some(NamedKey::F14),
        "f15" => Some(NamedKey::F15),
        "f16" => Some(NamedKey::F16),
        "f17" => Some(NamedKey::F17),
        "f18" => Some(NamedKey::F18),
        "f19" => Some(NamedKey::F19),
        "f20" => Some(NamedKey::F20),
        "f21" => Some(NamedKey::F21),
        "f22" => Some(NamedKey::F22),
        "f23" => Some(NamedKey::F23),
        "f24" => Some(NamedKey::F24),
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
    fn optional_capabilities_default_without_fake_implementations() {
        let (session, _) = crate::test_support::FakeTerminalSession::boxed(24, 80, "");
        let capabilities = session.capabilities();
        assert!(capabilities.network_stats.is_none());
        assert!(capabilities.sftp.is_none());
        assert!(capabilities.cwd_source.is_none());
    }

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

    #[test]
    fn bracketed_paste_strips_embedded_markers() {
        use crate::test_support::FakeTerminalSession;

        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        probe.set_mode(TermMode::SHOW_CURSOR | TermMode::BRACKETED_PASTE);

        // Embedded end marker must be stripped so it cannot terminate paste mode.
        let vectors = [
            ("", b"\x1b[200~\x1b[201~".as_slice()),
            (
                "line one\nline two",
                b"\x1b[200~line one\nline two\x1b[201~",
            ),
            (
                "unicode: Héllo, 世界",
                "\x1b[200~unicode: Héllo, 世界\x1b[201~".as_bytes(),
            ),
            (
                "nul:\0control:\u{0001}",
                b"\x1b[200~nul:\0control:\x01\x1b[201~",
            ),
            // The key fix: embedded ESC[201~ must be stripped.
            ("safe\x1b[201~malicious", b"\x1b[200~safemalicious\x1b[201~"),
            // Embedded start marker also stripped.
            ("text\x1b[200~more", b"\x1b[200~textmore\x1b[201~"),
            // Multiple embedded markers.
            ("a\x1b[201~b\x1b[200~c\x1b[201~d", b"\x1b[200~abcd\x1b[201~"),
            // Partial marker (no ~) is NOT stripped.
            ("text\x1b[201x", b"\x1b[200~text\x1b[201x\x1b[201~"),
        ];

        for (text, expected) in vectors {
            session.paste(text);
            let writes = probe.take_writes();
            assert_eq!(writes.len(), 1, "for text: {text:?}");
            assert_eq!(writes[0], expected, "for text: {text:?}");
        }
    }

    #[test]
    fn plain_paste_preserves_unicode_and_controls() {
        use crate::test_support::FakeTerminalSession;

        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let text = "Héllo\n世界\0\u{0001}";
        session.paste(text);
        assert_eq!(probe.writes(), vec![text.as_bytes().to_vec()]);
    }
}
