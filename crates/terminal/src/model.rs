//! Shared terminal-model adapter — eliminates duplication between the local
//! and SSH backends.
//!
//! Both `LocalSession` and `SshSession` wrap one [`SharedTerminal`] — the
//! `oneterm-vt` engine behind a fair mutex — and
//! implement the same terminal-model operations (snapshot, query, scroll,
//! selection, search, mouse encoding, etc.). This module provides a single
//! `TerminalModel` that both backends delegate to, so the logic lives in one
//! place.
//!
//! Every position is a `RowId` or a display row; since `US-0085` nothing here
//! publishes the reference's signed grid line.

use oneterm_vt::{ExtrasId, ModeSnapshot, MouseReporting, SelectionKind, Size, Terminal};

use crate::content::{LineRangeCells, SnapshotCell, TerminalContent};
use crate::handle::SharedTerminal;
use crate::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use crate::osc_color::DynamicColors;
use crate::{SearchMatch, SearchOptions, TerminalInfo, TerminalQueryState};
use oneterm_vt::search::{GridText, SearchPattern, search_grid_text};

/// Shared terminal-model operations backed by the engine.
///
/// Created per call by [`PtySession`](crate::session::PtySession) and stored
/// nowhere — it is two words wide. Every render-path state that must survive between frames —
/// the damage watermark and the copied rows — lives in the caller's
/// [`TerminalContent`], which is the buffer the renderer reuses, not in
/// anything this type or the lock holds.
pub struct TerminalModel {
    term: SharedTerminal,
    resize_policy: oneterm_vt::ResizePolicy,
}

impl TerminalModel {
    /// Wrap an existing [`SharedTerminal`].
    ///
    /// Takes anything that converts into the engine's policy; the backends pass
    /// `oneterm_vt::ResizePolicy` directly (`US-0091`).
    pub fn new(term: SharedTerminal, resize_policy: impl Into<oneterm_vt::ResizePolicy>) -> Self {
        Self {
            term,
            resize_policy: resize_policy.into(),
        }
    }

    /// Borrow the underlying handle (for external access).
    pub fn term(&self) -> &SharedTerminal {
        &self.term
    }

    // ── Render ──────────────────────────────────────────────────────

    /// Snapshot the grid for rendering.
    ///
    /// Allocates a fresh buffer, so it carries a fresh watermark and reports
    /// `Full`: it cannot steal the renderer's damage, which the shared render
    /// state it used to go through could.
    pub fn snapshot(&self) -> TerminalContent {
        let mut term = self.term.lock_for_render();
        TerminalContent::from(&mut term)
    }

    /// Refill the caller's frame buffer (advances **its** watermark).
    /// Reuses `out`'s allocations — zero steady-state allocation per frame.
    pub fn snapshot_into(&self, out: &mut TerminalContent) {
        let mut term = self.term.lock_for_render();
        out.refill(&mut term);
    }

    /// Compact query state — modes, cursor, viewport size (O(1)).
    pub fn query_state(&self, alive: bool) -> TerminalQueryState {
        let term = self.term.lock();
        let screen = term.screen();
        let cursor = screen.cursor();
        TerminalQueryState {
            modes: term.mode_snapshot(),
            cursor_row: cursor.pos.row.distance(screen.screen_top()) as usize,
            cursor_col: usize::from(cursor.pos.col),
            cursor_shape: term.cursor_style().shape,
            display_offset: screen.scroll_offset() as usize,
            rows: usize::from(screen.rows()),
            cols: usize::from(screen.cols()),
            total_lines: screen.history_len() as usize + usize::from(screen.rows()),
            alive,
        }
    }

    /// Read cells for a range of display lines (O(window×cols)).
    ///
    /// Damage-free: it reads the rows by `RowId` straight off the screen instead
    /// of going through a render state, so a URL hover or a completion lookup
    /// never consumes the renderer's damage.
    pub fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        line_range_cells(&self.term.lock(), start_line, count)
    }

    /// Dynamic OSC colors (foreground/background/cursor + 256 indexed).
    pub fn dynamic_colors(&self) -> DynamicColors {
        use oneterm_vt::ColorKey;

        let term = self.term.lock();
        let mut indexed = [None; 256];
        for (index, slot) in indexed.iter_mut().enumerate() {
            *slot = term.color(ColorKey::Palette(index as u8));
        }
        DynamicColors {
            foreground: term.color(ColorKey::Foreground),
            background: term.color(ColorKey::Background),
            cursor: term.color(ColorKey::Cursor),
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
            screen_top: screen.screen_top(),
            cursor_row: screen.cursor().pos.row.distance(screen.screen_top()) as usize,
            last_content_row: crate::last_content_row(&term),
            num_lines: usize::from(screen.rows()),
            num_cols: usize::from(screen.cols()),
            display_offset: screen.scroll_offset() as usize,
            clear_epoch,
        }
    }

    /// The modes the UI reads (mouse, alt-screen, bracketed paste…).
    pub fn modes(&self) -> ModeSnapshot {
        self.term.lock().mode_snapshot()
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
        term.resize(Size { rows, cols }, self.resize_policy);
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
    pub fn start_selection(&self, row: f32, col: f32, kind: SelectionKind) {
        let mut term = self.term.lock();
        let (pos, side) = term.hit_test(row, col);
        term.selection_start(pos, side, kind);
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
        search_grid_text(&text, SearchPattern::Literal(query), options)
    }

    // ── Mouse ──────────────────────────────────────────────────────

    /// Returns encoded mouse-press bytes if in mouse mode, otherwise starts a
    /// selection and returns `None`.
    pub fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        kind: SelectionKind,
        mods: MouseModifiers,
    ) -> Option<Vec<u8>> {
        let modes = self.modes();
        if modes.mouse.is_some() {
            let bytes = encode_mouse_press(row as usize, col as usize, button, modes, mods);
            Some(bytes)
        } else if matches!(button, TerminalMouseButton::Left) {
            self.start_selection(row, col, kind);
            None
        } else {
            None
        }
    }

    /// Returns encoded mouse-move bytes if in mouse motion/drag mode.
    pub fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers) -> Option<Vec<u8>> {
        let modes = self.modes();
        if reports_motion(modes) {
            let bytes = encode_mouse_move(row as usize, col as usize, None, modes, mods);
            Some(bytes)
        } else {
            None
        }
    }

    /// Returns encoded mouse-drag bytes if in mouse mode, otherwise updates
    /// the selection.
    pub fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers) -> Option<Vec<u8>> {
        let modes = self.modes();
        if modes.mouse.is_some() {
            let bytes = encode_mouse_move(
                row as usize,
                col as usize,
                Some(TerminalMouseButton::Left),
                modes,
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
        let modes = self.modes();
        if modes.mouse.is_some() {
            let bytes = encode_mouse_release(row as usize, col as usize, button, modes, mods);
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
        let display_offset = term.screen().scroll_offset();

        if display_offset > 0 {
            if !modes.alt_screen {
                scroll_viewport(&mut term, -scroll_delta);
            }
            None
        } else if modes.mouse.is_some() {
            let bytes = encode_wheel_event(row as usize, col as usize, delta_y, modes, mods);
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
fn scroll_viewport(term: &mut Terminal, delta: i32) {
    term.grid_mut().screen_mut().scroll_viewport(delta);
    term.grid_mut().sync_anchors();
}

/// Read cells for a range of display lines, straight off the screen.
///
/// Damage-free: it reads the rows by `RowId` instead of going through a render
/// state, so a URL hover or a completion lookup never consumes the renderer's
/// damage — and never copies a viewport to answer a pointer move.
pub(crate) fn line_range_cells(term: &Terminal, start_line: usize, count: usize) -> LineRangeCells {
    let screen = term.screen();
    let num_cols = usize::from(screen.cols());
    let num_lines = usize::from(screen.rows());
    if start_line >= num_lines || count == 0 {
        return LineRangeCells {
            num_cols,
            ..LineRangeCells::default()
        };
    }
    let actual_count = count.min(num_lines - start_line);
    let top = screen.visible_top();
    let interner = term.interner();
    let mut cells: Vec<SnapshotCell> = Vec::with_capacity(actual_count * num_cols);
    let mut links: Vec<(oneterm_vt::HyperlinkId, String)> = Vec::new();
    for index in 0..actual_count {
        let row = screen.row(top + (start_line + index) as u64);
        let wrapped = row.wrapped();
        let last = row.cells().len().saturating_sub(1);
        for (col, cell) in row.cells().iter().enumerate() {
            let hyperlink = (cell.extras_id() != ExtrasId::NONE)
                .then(|| interner.resolve_extras(cell.extras_id()).hyperlink)
                .flatten();
            if let Some(id) = hyperlink
                && !links.iter().any(|(known, _)| *known == id)
                && let Some(link) = interner.hyperlinks.resolve(id)
            {
                links.push((id, link.uri.to_string()));
            }
            cells.push(SnapshotCell {
                ch: cell.text_char(&interner.graphemes),
                spacer: cell.width().is_spacer(),
                wrapline: wrapped && col == last,
                hyperlink,
            });
        }
    }
    LineRangeCells {
        cells,
        num_cols,
        links,
    }
}

/// `? 1002` and `? 1003` report motion; `? 1000` reports presses only.
fn reports_motion(modes: ModeSnapshot) -> bool {
    matches!(
        modes.mouse.map(|mouse| mouse.reporting),
        Some(MouseReporting::ButtonEvent | MouseReporting::AnyEvent)
    )
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
