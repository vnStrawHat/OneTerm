//! `TerminalSession` — the session interface shared by the local shell and SSH,
//! composed from four focused traits (`TerminalRender`, `TerminalInput`,
//! `TerminalIme`, `TerminalLifecycle`) plus optional `TerminalCapabilities`.
//! The two backends implement it independently, unaware of each other.
//!
//! Pure: no GPUI dependency. Uses neutral types — the UI crate maps them to GPUI:
//! - `TerminalMouseButton` (instead of `gpui::MouseButton`).
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
use crate::backend::SharedState;
use crate::content::TerminalContent;
use crate::logging::TerminalLogController;
use crate::mouse_encode::{MouseModifiers, TerminalMouseButton};
use crate::osc::{Osc133Kind, TerminalProgress};
use crate::osc_agent::AgentStatusEvent;
use crate::osc_color::DynamicColors;
use crate::paste::{PasteMode, PastePolicy, PasteResult, encode_paste};
use crate::search::{SearchMatch, SearchOptions};

/// Error from a terminal input/control operation (write, resize, close).
///
/// Backends return this error at the transport boundary so callers can
/// distinguish saturation, closure, and transport failures instead of silently
/// dropping input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalError {
    /// The command queue is full — the caller must retry or report failure.
    QueueFull,
    /// The session/channel is closed — no more data can be sent.
    Closed,
    /// The PTY/SSH channel encountered a transport error.
    Transport(String),
}

impl std::fmt::Display for TerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull => write!(f, "terminal command queue is full"),
            Self::Closed => write!(f, "terminal session is closed"),
            Self::Transport(msg) => write!(f, "terminal transport error: {msg}"),
        }
    }
}

impl std::error::Error for TerminalError {}

/// Log a best-effort generated input failure with operation context.
///
/// User keystrokes use the typed [`TerminalInput::write`] result directly. Mouse
/// reports, clear commands, and IME commits currently have void trait methods, so
/// their delivery failures must remain observable rather than being discarded.
pub fn report_generated_input(operation: &str, result: Result<(), TerminalError>) {
    if let Err(error) = result {
        log::warn!("{operation} delivery failed: {error}");
    }
}

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
    /// Live OSC 7 working-directory state, if available — read on demand so the
    /// SFTP browser's "sync to terminal cwd" always sees the latest `cd`.
    pub cwd_source: Option<SharedState>,
    /// Printable-output logging controller for this terminal.
    pub logging: Option<TerminalLogController>,
}
/// Which backend a session talks to. Replaces the former `is_local() -> bool`
/// so call sites read as a domain concept (ARCH-44).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionKind {
    /// A local shell behind a PTY.
    Local,
    /// A remote shell over SSH.
    Ssh,
}

impl SessionKind {
    /// `true` for a local shell — for the few call sites that still hand the
    /// locality to a boolean API.
    pub const fn is_local(self) -> bool {
        matches!(self, Self::Local)
    }
}

/// Cells for a window of display lines — see [`TerminalRender::query_line_range_cells`].
#[derive(Debug, Clone, Default)]
pub struct LineRangeCells {
    /// Up to `count × num_cols` cells starting at the requested display line,
    /// in row-major order. Empty when the range starts below the viewport.
    pub cells: Vec<IndexedCell>,
    /// Viewport width in columns; the row stride of `cells`.
    pub num_cols: usize,
}

/// Why [`TerminalSession::paste`] did not deliver the text (ERR-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasteError {
    /// The payload exceeded the paste policy's byte limit; nothing was written.
    TooLarge {
        /// Size of the rejected payload in bytes.
        bytes: usize,
        /// The policy limit in bytes.
        max_bytes: usize,
    },
    /// The encoded bytes could not be written to the PTY/channel.
    Write(TerminalError),
}

impl std::fmt::Display for PasteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes, max_bytes } => write!(
                f,
                "paste rejected: {bytes} bytes exceed the {max_bytes}-byte limit"
            ),
            Self::Write(error) => write!(f, "paste failed: {error}"),
        }
    }
}

impl std::error::Error for PasteError {}

impl From<TerminalError> for PasteError {
    fn from(error: TerminalError) -> Self {
        Self::Write(error)
    }
}

/// Grid reads: render snapshots, damage-free queries, search, selection text
/// and the colour table.
///
/// Every method is required — a backend that cannot answer a query returns the
/// documented empty value (`Vec::new()`, `None`, `DynamicColors::default()`)
/// explicitly instead of inheriting a silent default.
pub trait TerminalRender: Send + Sync {
    /// Snapshot the grid for rendering (does not hold the lock while drawing).
    ///
    /// **Consumes and resets** the terminal damage — call this **only** from the
    /// render/prepaint path, exactly once per frame. For any other read
    /// (mouse hit-test, URL detection, mode checks) use
    /// [`query_state`](Self::query_state) or
    /// [`query_line_range_cells`](Self::query_line_range_cells), which do not
    /// touch damage and never clone the full grid.
    fn snapshot(&self) -> TerminalContent;

    /// [`snapshot`](Self::snapshot) into a reusable buffer — same contract
    /// (consumes damage; render path only, once per frame), but reuses `out`'s
    /// allocations so the steady-state render loop allocates nothing. The
    /// default falls back to `snapshot()` for simple implementations.
    fn snapshot_into(&self, out: &mut TerminalContent) {
        *out = self.snapshot();
    }

    /// Compact query state for non-render reads — mode, cursor, viewport size.
    /// Does NOT clone the full grid (O(1)). Use this for mode checks, cursor
    /// positioning, and viewport-size reads.
    fn query_state(&self) -> TerminalQueryState;

    /// Read cells for a range of display lines (0-based from top of viewport).
    /// O(window×cols), damage-free — used for URL hover detection and
    /// completion, where only a few lines are needed. There is deliberately no
    /// damage-free full-grid snapshot: every non-render read fits `query_state`
    /// or a line range, and an O(rows×cols) clone per event is a footgun.
    fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells;

    /// Basic info (total_lines, cursor_line) — does NOT call damage()/reset_damage().
    /// Used for line_times updates without clearing damage for prepaint.
    fn terminal_info(&self) -> TerminalInfo;

    /// Alt-screen is on (e.g. vim/less) → disable IME, plain keys go through on_key_down.
    fn is_alt_screen(&self) -> bool;

    /// Dynamic OSC-set foreground/background/cursor colors (OSC 10/11/12).
    /// Read from the live `Term` color table so the renderer can apply them on
    /// top of the theme. `DynamicColors::default()` = none set (use the theme).
    fn dynamic_colors(&self) -> DynamicColors;

    /// Provide the theme's default colors: foreground/background/cursor plus the
    /// 16-color ANSI palette. Used to answer OSC 10/11/12 and OSC 4 *queries*
    /// when the color was never set via OSC (so a bare query still reports a
    /// sensible color, e.g. for background detection). Called by the UI whenever
    /// the theme changes.
    fn set_default_colors(&self, foreground: Rgb, background: Rgb, cursor: Rgb, ansi: [Rgb; 16]);

    /// Search the full scrollback + viewport for `query` and return matches in
    /// grid coordinates (top-to-bottom order). Empty query → empty result.
    ///
    /// Backends snapshot the grid text under the `Term` lock and match outside
    /// it ([`crate::search`]).
    fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchMatch>;

    /// The currently selected text (for copy). `None` if there is no selection.
    fn selection_text(&self) -> Option<String>;

    /// Whether a non-empty selection exists — O(1), does not materialise the
    /// selected text (PERF-14). Use it for enabling "Copy" in menus.
    fn has_selection(&self) -> bool;
}

/// Bytes into the PTY/channel plus viewport, mouse and selection manipulation.
pub trait TerminalInput: Send + Sync {
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

    /// Clear the current selection.
    fn clear_selection(&self);
    /// Select all content (scrollback + visible).
    fn select_all(&self);
    /// Clear screen + scrollback (send a clear escape sequence to the PTY).
    fn clear(&self);
}

/// IME composition state (marked text) and commit.
pub trait TerminalIme: Send + Sync {
    fn set_marked_text(&self, text: String);
    fn clear_marked_text(&self);
    fn commit_text(&self, text: &str);
    fn marked_text(&self) -> Option<String>;
}

/// Events, liveness, close, identity.
pub trait TerminalLifecycle: Send + Sync {
    /// Take the session's single event receiver.
    ///
    /// Sessions have exactly one event consumer: the backend owns one bounded
    /// channel and hands its receiver out once. The first call returns
    /// `Some(receiver)`; every later call returns `None`, so a second consumer
    /// cannot silently starve the first one. Callers treat `None` as a
    /// programming error (the view that owns the session already took it).
    fn take_events(&self) -> Option<Receiver<SessionEvent>>;
    /// Whether the process is still alive (not exited/closed).
    fn alive(&self) -> bool;
    /// Close the session (shut down PTY / close channel).
    fn close(&self) -> Result<(), TerminalError>;
    /// Which backend this session talks to.
    fn kind(&self) -> SessionKind;
    /// The current title (OSC 0/2).
    fn title(&self) -> Option<String>;
    /// The current cwd (OSC 7).
    fn cwd(&self) -> Option<PathBuf>;
}

/// A terminal session as seen by the UI: the four focused traits composed into
/// one object-safe façade, plus derived helpers built only on those traits and
/// the optional [`TerminalCapabilities`].
///
/// Snapshot + input + lifecycle only — does **not** force a shared pump/transport.
/// `LocalSession` (alacritty tty + EventLoop) and `SshSession` (russh) implement
/// it independently. The two backends do not depend on each other.
pub trait TerminalSession:
    TerminalRender + TerminalInput + TerminalIme + TerminalLifecycle + 'static
{
    /// Return capabilities provided by this backend without widening the stable
    /// session façade for every future optional feature. Local sessions return
    /// the empty default.
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities::default()
    }

    // ── Derived helpers — pure functions of the traits above ─────────────

    /// Send raw text to the PTY (for automation, extensions, task runners).
    /// Equivalent to Zed `SendText(String)`.
    fn send_text(&self, text: &str) {
        if let Err(error) = self.write(text.as_bytes()) {
            log::warn!("terminal send_text failed: {error}");
        }
    }

    /// Bracketed paste mode is on → wrap the paste in `\x1b[200~...\x1b[201~`.
    /// Zed: checks `Modes::BRACKETED_PASTE` then wraps.
    fn is_bracketed_paste(&self) -> bool {
        self.query_state().mode.contains(TermMode::BRACKETED_PASTE)
    }

    /// Paste text into the PTY. Automatically wraps it in bracketed paste markers
    /// if the terminal is in bracketed paste mode.
    ///
    /// A rejected (too large) or undeliverable paste is returned to the caller
    /// so the UI can tell the user (ERR-04) — nothing is written in that case.
    fn paste(&self, text: &str) -> Result<(), PasteError> {
        let mode = if self.is_bracketed_paste() {
            PasteMode::Bracketed
        } else {
            PasteMode::Plain
        };
        let policy = PastePolicy::default();
        match encode_paste(text, mode, &policy) {
            PasteResult::Ok(bytes) => Ok(self.write(&bytes)?),
            PasteResult::TooLarge(bytes) => Err(PasteError::TooLarge {
                bytes,
                max_bytes: policy.max_bytes,
            }),
        }
    }
}

/// Implement [`TerminalRender`], [`TerminalInput`], [`TerminalIme`] and
/// [`TerminalLifecycle`] for a backend session that owns the standard fields
/// (`term`, `listener`, `state`, `marked_text`, `event_rx`).
///
/// The local shell and SSH sessions differ only in their
/// [`TerminalCapabilities`], their [`SessionKind`] and how the channel is torn
/// down, so those three stay with the backend (`$kind` and `$close`, an
/// inherent method returning `Result<(), TerminalError>`) and everything else
/// lives here once. See `docs/terminal-backend.md` §9.
#[macro_export]
macro_rules! impl_pty_terminal_session {
    ($ty:ty, $listener:ty, $label:literal, $kind:expr, $close:ident) => {
        impl $ty {
            /// A `TerminalModel` adapter for the shared terminal-model
            /// operations. Cheap — just wraps the existing `Arc<FairMutex<Term>>`.
            fn model(&self) -> $crate::model::TerminalModel<$listener> {
                $crate::model::TerminalModel::new(self.term.clone())
            }

            /// Write bytes to the PTY / SSH channel while the session is alive.
            fn pty_write(&self, bytes: &[u8]) -> Result<(), $crate::TerminalError> {
                if !self.state.alive() {
                    return Err($crate::TerminalError::Closed);
                }
                $crate::backend::PtyTransport::pty_write(self.listener.transport(), bytes)
            }
        }

        impl $crate::TerminalRender for $ty {
            fn snapshot(&self) -> $crate::TerminalContent {
                self.model().snapshot()
            }

            fn snapshot_into(&self, out: &mut $crate::TerminalContent) {
                self.model().snapshot_into(out)
            }

            fn query_state(&self) -> $crate::TerminalQueryState {
                self.model()
                    .query_state($crate::TerminalLifecycle::alive(self))
            }

            fn query_line_range_cells(
                &self,
                start_line: usize,
                count: usize,
            ) -> $crate::LineRangeCells {
                self.model().query_line_range_cells(start_line, count)
            }

            fn dynamic_colors(&self) -> $crate::DynamicColors {
                self.model().dynamic_colors()
            }

            fn set_default_colors(
                &self,
                foreground: ::alacritty_terminal::vte::ansi::Rgb,
                background: ::alacritty_terminal::vte::ansi::Rgb,
                cursor: ::alacritty_terminal::vte::ansi::Rgb,
                ansi: [::alacritty_terminal::vte::ansi::Rgb; 16],
            ) {
                self.state.set_default_colors($crate::DefaultColors {
                    foreground: Some(foreground),
                    background: Some(background),
                    cursor: Some(cursor),
                    ansi: Some(ansi),
                });
            }

            fn terminal_info(&self) -> $crate::TerminalInfo {
                self.model()
                    .terminal_info(self.state.absolute_line_count(), self.state.clear_epoch())
            }

            fn is_alt_screen(&self) -> bool {
                self.model().is_alt_screen()
            }

            fn search(
                &self,
                query: &str,
                options: $crate::SearchOptions,
            ) -> Vec<$crate::SearchMatch> {
                self.model().search(query, options)
            }

            fn selection_text(&self) -> Option<String> {
                self.model().selection_text()
            }

            fn has_selection(&self) -> bool {
                self.model().has_selection()
            }
        }

        impl $crate::TerminalInput for $ty {
            fn write(&self, bytes: &[u8]) -> Result<(), $crate::TerminalError> {
                self.pty_write(bytes)
            }

            /// Send a DSR (Device Status Report) query so the terminal answers
            /// with the cursor position. Windows ConPTY buffers output and only
            /// flushes on interaction; SSH simply ignores the round trip.
            fn flush_pty(&self) {
                if let Err(error) = self.pty_write(b"\x1b[6n") {
                    log::warn!(concat!($label, ": PTY flush query failed: {}"), error);
                }
            }

            /// Write ETX (`\x03`); the shell's line discipline (or ConPTY, with
            /// OpenConsole.exe next to the exe) turns it into the interrupt so
            /// only the child process sees it.
            fn send_ctrl_c(&self) {
                if let Err(error) = self.pty_write(b"\x03") {
                    log::warn!(concat!($label, ": Ctrl+C delivery failed: {}"), error);
                }
            }

            fn resize(&self, rows: u16, cols: u16) -> Result<(), $crate::TerminalError> {
                if self.model().needs_resize(rows, cols) {
                    $crate::backend::PtyTransport::pty_resize(
                        self.listener.transport(),
                        rows,
                        cols,
                    )?;
                    self.model().resize_grid(rows, cols);
                }
                Ok(())
            }

            fn scroll(&self, delta: i32) {
                self.model().scroll(delta);
            }

            fn scroll_to_bottom(&self) {
                self.model().scroll_to_bottom();
            }

            fn scroll_to_top(&self) {
                self.model().scroll_to_top();
            }

            fn mouse_down(
                &self,
                row: f32,
                col: f32,
                button: $crate::TerminalMouseButton,
                sel: ::alacritty_terminal::selection::SelectionType,
                mods: $crate::MouseModifiers,
            ) {
                if let Some(bytes) = self.model().mouse_down(row, col, button, sel, mods) {
                    $crate::report_generated_input(
                        concat!($label, " mouse input"),
                        self.pty_write(&bytes),
                    );
                }
            }

            fn mouse_move(&self, row: f32, col: f32, mods: $crate::MouseModifiers) {
                if let Some(bytes) = self.model().mouse_move(row, col, mods) {
                    $crate::report_generated_input(
                        concat!($label, " mouse input"),
                        self.pty_write(&bytes),
                    );
                }
            }

            fn mouse_drag(&self, row: f32, col: f32, mods: $crate::MouseModifiers) {
                if let Some(bytes) = self.model().mouse_drag(row, col, mods) {
                    $crate::report_generated_input(
                        concat!($label, " mouse input"),
                        self.pty_write(&bytes),
                    );
                }
            }

            fn mouse_up(
                &self,
                row: f32,
                col: f32,
                button: $crate::TerminalMouseButton,
                mods: $crate::MouseModifiers,
            ) {
                if let Some(bytes) = self.model().mouse_up(row, col, button, mods) {
                    $crate::report_generated_input(
                        concat!($label, " mouse input"),
                        self.pty_write(&bytes),
                    );
                }
            }

            fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: $crate::MouseModifiers) {
                if let Some(bytes) = self.model().wheel(delta_y, row, col, mods) {
                    $crate::report_generated_input(
                        concat!($label, " mouse input"),
                        self.pty_write(&bytes),
                    );
                }
            }

            fn clear_selection(&self) {
                self.model().clear_selection();
            }

            fn select_all(&self) {
                self.model().select_all();
            }

            fn clear(&self) {
                // Send the `clear` command to the shell, exactly as if the user typed it.
                $crate::report_generated_input(
                    concat!($label, " clear command"),
                    self.pty_write(b"clear\r"),
                );
                $crate::TerminalInput::clear_selection(self);
            }
        }

        impl $crate::TerminalIme for $ty {
            fn set_marked_text(&self, text: String) {
                *self.marked_text.lock().unwrap() = Some(text);
            }

            fn clear_marked_text(&self) {
                *self.marked_text.lock().unwrap() = None;
            }

            fn commit_text(&self, text: &str) {
                $crate::TerminalIme::clear_marked_text(self);
                $crate::report_generated_input(
                    concat!($label, " committed text"),
                    self.pty_write(text.as_bytes()),
                );
            }

            fn marked_text(&self) -> Option<String> {
                self.marked_text.lock().unwrap().clone()
            }
        }

        impl $crate::TerminalLifecycle for $ty {
            fn take_events(&self) -> Option<::async_channel::Receiver<$crate::SessionEvent>> {
                self.event_rx.lock().unwrap().take()
            }

            fn alive(&self) -> bool {
                self.state.alive()
            }

            fn close(&self) -> Result<(), $crate::TerminalError> {
                let result = self.$close();
                self.state.set_alive(false);
                result
            }

            fn kind(&self) -> $crate::SessionKind {
                $kind
            }

            fn title(&self) -> Option<String> {
                self.state.title()
            }

            fn cwd(&self) -> Option<::std::path::PathBuf> {
                self.state.cwd()
            }
        }
    };
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
    fn take_events_hands_out_the_receiver_once() {
        let (session, probe) = crate::test_support::FakeTerminalSession::boxed(24, 80, "");
        let events = session
            .take_events()
            .expect("first take_events returns the receiver");
        assert!(session.take_events().is_none());

        // The receiver taken first is the live one.
        probe.emit(SessionEvent::Bell).unwrap();
        assert!(matches!(events.try_recv(), Ok(SessionEvent::Bell)));
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
            // Partial marker (no ~): the ESC is still stripped.
            ("text\x1b[201x", b"\x1b[200~text[201x\x1b[201~"),
            // SEC-01: nested marker must not reassemble into a terminator.
            ("\x1b[20\x1b[201~1~", b"\x1b[200~[201~\x1b[201~"),
            // ETX is stripped (some shells end bracketed paste on it).
            ("a\x03b", b"\x1b[200~ab\x1b[201~"),
        ];

        for (text, expected) in vectors {
            session.paste(text).unwrap();
            let writes = probe.take_writes();
            assert_eq!(writes.len(), 1, "for text: {text:?}");
            assert_eq!(writes[0], expected, "for text: {text:?}");
        }
    }

    #[test]
    fn paste_over_the_size_limit_is_reported_and_not_written() {
        use crate::test_support::FakeTerminalSession;

        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        let max_bytes = PastePolicy::default().max_bytes;
        let huge = "x".repeat(max_bytes + 1);
        assert_eq!(
            session.paste(&huge),
            Err(PasteError::TooLarge {
                bytes: max_bytes + 1,
                max_bytes
            })
        );
        assert!(probe.writes().is_empty());
    }

    #[test]
    fn paste_write_failure_is_reported() {
        use crate::test_support::FakeTerminalSession;

        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        probe.fail_writes(true);
        assert_eq!(
            session.paste("hi"),
            Err(PasteError::Write(TerminalError::QueueFull))
        );
    }

    #[test]
    fn session_kind_locality() {
        assert!(SessionKind::Local.is_local());
        assert!(!SessionKind::Ssh.is_local());
        let (session, _) = crate::test_support::FakeTerminalSession::boxed(24, 80, "");
        assert_eq!(session.kind(), SessionKind::Local);
    }

    #[test]
    fn plain_paste_preserves_unicode_and_controls() {
        use crate::test_support::FakeTerminalSession;

        let (session, probe) = FakeTerminalSession::boxed(24, 80, "");
        // Plain (non-bracketed) paste keeps unicode/controls but rewrites LF → CR.
        let text = "Héllo\n世界\0\u{0001}";
        session.paste(text).unwrap();
        assert_eq!(
            probe.writes(),
            vec!["Héllo\r世界\0\u{0001}".as_bytes().to_vec()]
        );
    }
}
