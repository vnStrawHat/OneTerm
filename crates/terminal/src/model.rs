//! Shared terminal-model adapter — eliminates duplication between the local
//! and SSH backends.
//!
//! Both `LocalSession` and `SshSession` wrap one [`SharedTerminal`] — the
//! `oneterm-vt` engine plus its legacy render state behind a fair mutex — and
//! implement the same terminal-model operations (snapshot, query, scroll,
//! selection, search, mouse encoding, etc.). This module provides a single
//! `TerminalModel` that both backends delegate to, so the logic lives in one
//! place.
//!
//! Coordinates leaving this module are still the reference's: `cursor_line`,
//! `SearchMatch::line` and `SelectionRange`'s ends are grid lines where `0` is
//! the viewport top at `display_offset == 0` and negative values are
//! scrollback. The engine has no negative rows; the conversion lives in
//! [`crate::engine_shim`] and `US-0082` deletes it.

use alacritty_terminal::index::Line;
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use oneterm_vt::{SelectionKind, Size};

use crate::content::TerminalContent;
use crate::engine::SharedTerminal;
use crate::engine_shim::{
    Placements, legacy_cursor_shape, legacy_mode, legacy_rgb, push_grid_row, row_to_line,
};
use crate::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use crate::osc_color::DynamicColors;
use crate::search::{GridText, search_grid_text};
use crate::{
    IndexedCell, LineRangeCells, SearchMatch, SearchOptions, TerminalInfo, TerminalQueryState,
};

/// How a grow-resize treats the primary grid's scrollback (DEC-0008).
///
/// Chosen by the backend that owns the PTY, because it must agree with whatever
/// sits on the other side of that PTY. Both are native engine policies since
/// `US-0077`; this enum survives only so the backends' `resize_policy()` — which
/// `US-0083` / `US-0084` own — keeps compiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizePolicy {
    /// The reference's behaviour: added rows are pulled from scrollback into the
    /// top of the viewport and the cursor moves down by the same amount. Right
    /// for Unix PTYs and SSH, where the remote side reflows and repaints.
    #[default]
    Default,
    /// Existing rows keep their index, added rows are blank at the bottom, the
    /// cursor row is unchanged and scrollback is untouched. Matches conhost
    /// behind ConPTY, which keeps its viewport top on a resize and addresses
    /// later output with absolute cursor positions in its own coordinates.
    KeepViewportTop,
}

impl From<ResizePolicy> for oneterm_vt::ResizePolicy {
    fn from(policy: ResizePolicy) -> oneterm_vt::ResizePolicy {
        match policy {
            ResizePolicy::Default => oneterm_vt::ResizePolicy::BottomAnchor,
            ResizePolicy::KeepViewportTop => oneterm_vt::ResizePolicy::KeepViewportTop,
        }
    }
}

/// Shared terminal-model operations backed by the engine.
///
/// Created per call by `impl_pty_terminal_session!` and stored nowhere — it is
/// two words wide. Every render-path state that must survive between frames
/// (the damage watermark, the copied rows) lives inside the lock, on
/// [`crate::engine::Engine`].
pub struct TerminalModel {
    term: SharedTerminal,
    resize_policy: ResizePolicy,
}

impl TerminalModel {
    /// Wrap an existing [`SharedTerminal`].
    pub fn new(term: SharedTerminal, resize_policy: ResizePolicy) -> Self {
        Self {
            term,
            resize_policy,
        }
    }

    /// Borrow the underlying handle (for external access).
    pub fn term(&self) -> &SharedTerminal {
        &self.term
    }

    // ── Render ──────────────────────────────────────────────────────

    /// Snapshot the grid for rendering (consumes damage).
    pub fn snapshot(&self) -> TerminalContent {
        let mut term = self.term.lock();
        TerminalContent::from(&mut term)
    }

    /// Snapshot the grid for rendering into a reusable buffer (consumes damage).
    /// Reuses `out`'s allocations — zero steady-state allocation per frame.
    pub fn snapshot_into(&self, out: &mut TerminalContent) {
        let mut term = self.term.lock();
        out.refill(&mut term);
    }

    /// Compact query state — mode, cursor, viewport size (O(1)).
    pub fn query_state(&self, alive: bool) -> TerminalQueryState {
        let term = self.term.lock();
        let screen = term.screen();
        let cursor = screen.cursor();
        TerminalQueryState {
            mode: legacy_mode(term.mode_snapshot()),
            cursor_line: row_to_line(cursor.pos.row, screen.screen_top()),
            cursor_col: usize::from(cursor.pos.col),
            cursor_shape: legacy_cursor_shape(term.cursor_style().shape),
            display_offset: screen.scroll_offset() as usize,
            rows: usize::from(screen.rows()),
            cols: usize::from(screen.cols()),
            total_lines: screen.history_len() as usize + usize::from(screen.rows()),
            alive,
        }
    }

    /// Read cells for a range of display lines (O(window×cols)).
    ///
    /// Damage-free: it reads the grid directly instead of going through the
    /// render state, so a URL hover or a completion lookup never consumes the
    /// renderer's damage.
    pub fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        let term = self.term.lock();
        let screen = term.screen();
        let num_cols = usize::from(screen.cols());
        let num_lines = usize::from(screen.rows());
        if start_line >= num_lines || count == 0 {
            return LineRangeCells {
                cells: Vec::new(),
                num_cols,
            };
        }
        let actual_count = count.min(num_lines - start_line);
        let offset = screen.scroll_offset() as usize;
        let top = screen.visible_top();
        let placements = Placements::new(&term);
        let mut cells: Vec<IndexedCell> = Vec::with_capacity(actual_count * num_cols);
        for index in 0..actual_count {
            let display_line = start_line + index;
            let line = Line(display_line as i32 - offset as i32);
            let row = screen.row(top + display_line as u64);
            push_grid_row(row, term.interner(), &placements, line, &mut cells);
        }
        LineRangeCells { cells, num_cols }
    }

    /// Dynamic OSC colors (foreground/background/cursor + 256 indexed).
    pub fn dynamic_colors(&self) -> DynamicColors {
        use oneterm_vt::ColorKey;

        let term = self.term.lock();
        let mut indexed = [None; 256];
        for (index, slot) in indexed.iter_mut().enumerate() {
            *slot = term.color(ColorKey::Palette(index as u8)).map(legacy_rgb);
        }
        DynamicColors {
            foreground: term.color(ColorKey::Foreground).map(legacy_rgb),
            background: term.color(ColorKey::Background).map(legacy_rgb),
            cursor: term.color(ColorKey::Cursor).map(legacy_rgb),
            indexed,
        }
    }

    /// Terminal info (total_lines, cursor, display_offset, etc.).
    /// `absolute_line_count` and `clear_epoch` come from the session's state.
    pub fn terminal_info(&self, absolute_line_count: usize, clear_epoch: usize) -> TerminalInfo {
        let term = self.term.lock();
        let screen = term.screen();
        let total_lines = screen.history_len() as usize + usize::from(screen.rows());
        TerminalInfo {
            total_lines,
            absolute_line_count: absolute_line_count.max(total_lines),
            cursor_line: row_to_line(screen.cursor().pos.row, screen.screen_top()),
            last_content_line: crate::last_content_line(&term),
            num_lines: usize::from(screen.rows()),
            num_cols: usize::from(screen.cols()),
            display_offset: screen.scroll_offset() as usize,
            clear_epoch,
        }
    }

    /// Current terminal mode flags.
    pub fn mode(&self) -> TermMode {
        legacy_mode(self.term.lock().mode_snapshot())
    }

    /// Whether alt-screen mode is active (vim/less/etc.).
    pub fn is_alt_screen(&self) -> bool {
        self.term.lock().mode_snapshot().alt_screen
    }

    /// Whether the program enabled any mouse-reporting mode.
    pub fn is_mouse_mode(&self) -> bool {
        self.term.lock().mouse_reporting().is_some()
    }

    // ── Resize / scroll ────────────────────────────────────────────

    /// Check whether the terminal grid needs resizing (without performing it).
    /// The caller should call `pty_resize` **before** `resize_grid` to ensure
    /// the PTY/process knows the new size before output arrives.
    pub fn needs_resize(&self, rows: u16, cols: u16) -> bool {
        let term = self.term.lock();
        let screen = term.screen();
        screen.cols() != cols || screen.rows() != rows
    }

    /// Actually resize the terminal grid. Should be called **after** `pty_resize`.
    ///
    /// Follows the session's [`ResizePolicy`], which is now the engine's own:
    /// the 61 lines of grid surgery the adapter used to run on top of the
    /// reference's bottom-anchored resize are gone (P13).
    pub fn resize_grid(&self, rows: u16, cols: u16) {
        let mut term = self.term.lock();
        term.resize(Size { rows, cols }, self.resize_policy.into());
    }

    /// Scroll the scrollback by `delta` lines (no-op in alt-screen).
    ///
    /// Positive scrolls towards history, which is the sign every caller already
    /// uses; the engine counts the other way, so the delta is negated once here.
    pub fn scroll(&self, delta: i32) {
        let mut term = self.term.lock();
        if !term.mode_snapshot().alt_screen {
            scroll_viewport(&mut term, -delta);
        }
    }

    /// Scroll to the bottom of the scrollback.
    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        if !term.mode_snapshot().alt_screen {
            term.grid_mut().screen_mut().scroll_to_bottom();
            term.grid_mut().sync_anchors();
        }
    }

    /// Scroll to the top of the scrollback.
    pub fn scroll_to_top(&self) {
        let mut term = self.term.lock();
        if !term.mode_snapshot().alt_screen {
            let history = term.screen().history_len() as i32;
            scroll_viewport(&mut term, history);
        }
    }

    // ── Selection ──────────────────────────────────────────────────

    /// Start a new selection at (row, col).
    pub fn start_selection(&self, row: f32, col: f32, sel: SelectionType) {
        let mut term = self.term.lock();
        let (pos, side) = term.hit_test(row, col);
        term.selection_start(pos, side, selection_kind(sel));
    }

    /// Update the existing selection end point (while dragging).
    pub fn update_selection(&self, row: f32, col: f32) {
        let mut term = self.term.lock();
        let (pos, side) = term.hit_test(row, col);
        term.selection_update(pos, side);
    }

    /// Get the selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_text()
    }

    /// Whether a non-empty selection exists (no text materialised — PERF-14).
    pub fn has_selection(&self) -> bool {
        self.term.lock().has_selection()
    }

    /// Clear the current selection.
    pub fn clear_selection(&self) {
        self.term.lock().selection_clear();
    }

    /// Select the entire scrollback.
    pub fn select_all(&self) {
        self.term.lock().select_all();
    }

    // ── Search ─────────────────────────────────────────────────────

    /// Search the terminal grid for `query`.
    ///
    /// The grid text is copied under the lock and matched after the lock is
    /// released, so a long scrollback search never stalls the pump (PERF-04).
    pub fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let text = {
            let term = self.term.lock();
            GridText::from_terminal(&term)
        };
        search_grid_text(&text, query, options)
    }

    // ── Mouse ──────────────────────────────────────────────────────

    /// Returns encoded mouse-press bytes if in mouse mode, otherwise starts a
    /// selection and returns `None`.
    pub fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        sel: SelectionType,
        mods: MouseModifiers,
    ) -> Option<Vec<u8>> {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = encode_mouse_press(row as usize, col as usize, button, mode, mods);
            Some(bytes)
        } else if matches!(button, TerminalMouseButton::Left) {
            self.start_selection(row, col, sel);
            None
        } else {
            None
        }
    }

    /// Returns encoded mouse-move bytes if in mouse motion/drag mode.
    pub fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers) -> Option<Vec<u8>> {
        let mode = self.mode();
        if mode.contains(TermMode::MOUSE_MOTION) || mode.contains(TermMode::MOUSE_DRAG) {
            let bytes = encode_mouse_move(row as usize, col as usize, None, mode, mods);
            Some(bytes)
        } else {
            None
        }
    }

    /// Returns encoded mouse-drag bytes if in mouse mode, otherwise updates
    /// the selection.
    pub fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers) -> Option<Vec<u8>> {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = encode_mouse_move(
                row as usize,
                col as usize,
                Some(TerminalMouseButton::Left),
                mode,
                mods,
            );
            Some(bytes)
        } else {
            self.update_selection(row, col);
            None
        }
    }

    /// Returns encoded mouse-release bytes if in mouse mode.
    pub fn mouse_up(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        mods: MouseModifiers,
    ) -> Option<Vec<u8>> {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = encode_mouse_release(row as usize, col as usize, button, mode, mods);
            Some(bytes)
        } else {
            None
        }
    }

    /// Wheel scroll — returns encoded bytes to write (if any), otherwise
    /// performs the scroll directly. Returns `None` when the scroll was
    /// handled internally (no bytes to write).
    pub fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers) -> Option<Vec<u8>> {
        let lines = (delta_y.abs().ceil() as i32).clamp(1, 10);
        let scroll_delta = if delta_y > 0.0 { lines } else { -lines };

        // One lock for the whole decision (PERF-17): mode + offset are read
        // and the scroll applied without releasing it in between.
        let mut term = self.term.lock();
        let modes = term.mode_snapshot();
        let mode = legacy_mode(modes);
        let display_offset = term.screen().scroll_offset();

        if display_offset > 0 {
            if !modes.alt_screen {
                scroll_viewport(&mut term, -scroll_delta);
            }
            None
        } else if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = encode_wheel_event(row as usize, col as usize, delta_y, mode, mods);
            Some(bytes)
        } else if modes.alt_screen {
            let key = match (delta_y > 0.0, modes.app_cursor) {
                (true, true) => "\x1bOA",
                (true, false) => "\x1b[A",
                (false, true) => "\x1bOB",
                (false, false) => "\x1b[B",
            };
            let mut bytes = Vec::new();
            for _ in 0..lines {
                bytes.extend_from_slice(key.as_bytes());
            }
            Some(bytes)
        } else {
            scroll_viewport(&mut term, -scroll_delta);
            None
        }
    }
}

/// Move the viewport and write the move back into the tracked-anchor list.
///
/// `Screen::scroll_viewport` updates the cached offset only; the anchor entry
/// is refreshed at the end of a `feed`, and a resize arriving before the next
/// one would otherwise read a stale viewport top.
fn scroll_viewport(term: &mut oneterm_vt::Terminal, delta: i32) {
    term.grid_mut().screen_mut().scroll_viewport(delta);
    term.grid_mut().sync_anchors();
}

fn selection_kind(sel: SelectionType) -> SelectionKind {
    match sel {
        SelectionType::Simple => SelectionKind::Simple,
        SelectionType::Block => SelectionKind::Block,
        SelectionType::Semantic => SelectionKind::Semantic,
        SelectionType::Lines => SelectionKind::Lines,
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;

/// The ConPTY resize contract, still pinned against the **old** engine.
///
/// R-44: the ten `keep_viewport_top_*` / `default_grow_*` tests in
/// [`legacy_resize`] are the only written form of what conhost does behind
/// ConPTY, so they keep running against `alacritty_terminal` until `US-0082`
/// deletes them — by which time the engine's own `reflow::tests::*` have been
/// green in the same commit. Nothing in the product path calls into this
/// module; it exists so the independent check survives the flip.
#[cfg(test)]
#[path = "legacy_resize.rs"]
mod legacy_resize;
