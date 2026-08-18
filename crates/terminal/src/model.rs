//! Shared terminal-model adapter — eliminates duplication between the local
//! and SSH backends.
//!
//! Both `LocalSession` and `SshSession` wrap an `alacritty_terminal::Term<EP>`
//! behind a `FairMutex` and implement the same terminal-model operations
//! (snapshot, query, scroll, selection, search, mouse encoding, etc.).
//! This module provides a single `TerminalModel<EP>` that both backends
//! delegate to, so the logic lives in one place.

use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::{Term, event::EventListener};

use crate::content::TerminalContent;
use crate::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use crate::search::{GridText, search_grid_text};
use crate::{
    BACKGROUND_INDEX, CURSOR_INDEX, DynamicColors, FOREGROUND_INDEX, IndexedCell, LineRangeCells,
    SearchMatch, SearchOptions, TerminalInfo, TerminalQueryState,
};

/// Simple grid dimensions for `Term::resize`.
struct TerminalSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Shared terminal-model operations backed by an `alacritty_terminal::Term`.
///
/// Created once per session and stored inside the `LocalSession` / `SshSession`
/// struct. All methods are identical across backends — the only differences
/// (transport: PTY vs SSH channel, lifecycle, state fields) remain on the
/// session structs themselves.
pub struct TerminalModel<EP: EventListener> {
    term: Arc<FairMutex<Term<EP>>>,
}

impl<EP: EventListener> TerminalModel<EP> {
    /// Wrap an existing `Arc<FairMutex<Term<EP>>>`.
    pub fn new(term: Arc<FairMutex<Term<EP>>>) -> Self {
        Self { term }
    }

    /// Borrow the underlying `Arc<FairMutex<Term<EP>>>` (for external access).
    pub fn term(&self) -> &Arc<FairMutex<Term<EP>>> {
        &self.term
    }

    // ── Render ──────────────────────────────────────────────────────

    /// Snapshot the grid for rendering (consumes damage).
    pub fn snapshot(&self) -> TerminalContent {
        let mut term = self.term.lock();
        TerminalContent::from(&mut *term)
    }

    /// Snapshot for auxiliary reads (does NOT consume damage).
    pub fn snapshot_query(&self) -> TerminalContent {
        let term = self.term.lock();
        TerminalContent::from_query(&*term)
    }

    /// Compact query state — mode, cursor, viewport size (O(1)).
    pub fn query_state(&self, alive: bool) -> TerminalQueryState {
        let term = self.term.lock();
        let content = term.renderable_content();
        TerminalQueryState {
            mode: content.mode,
            cursor_line: content.cursor.point.line.0,
            cursor_col: content.cursor.point.column.0,
            cursor_shape: content.cursor.shape,
            display_offset: content.display_offset,
            rows: term.screen_lines(),
            cols: term.columns(),
            total_lines: term.total_lines(),
            alive,
        }
    }

    /// Read cells for a range of display lines (O(window×cols)).
    pub fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        let term = self.term.lock();
        let num_cols = term.columns();
        let num_lines = term.screen_lines();
        if start_line >= num_lines || count == 0 {
            return LineRangeCells {
                cells: Vec::new(),
                num_cols,
            };
        }
        let actual_count = count.min(num_lines - start_line);
        let content = term.renderable_content();
        let start = start_line * num_cols;
        let end = start + actual_count * num_cols;
        let cells: Vec<IndexedCell> = content
            .display_iter
            .skip(start)
            .take(end - start)
            .map(|indexed| IndexedCell {
                point: indexed.point,
                cell: indexed.cell.clone(),
            })
            .collect();
        LineRangeCells { cells, num_cols }
    }

    /// Dynamic OSC colors (foreground/background/cursor + 256 indexed).
    pub fn dynamic_colors(&self) -> DynamicColors {
        let term = self.term.lock();
        let colors = term.colors();
        let mut indexed = [None; 256];
        for (i, slot) in indexed.iter_mut().enumerate() {
            *slot = colors[i];
        }
        DynamicColors {
            foreground: colors[FOREGROUND_INDEX],
            background: colors[BACKGROUND_INDEX],
            cursor: colors[CURSOR_INDEX],
            indexed,
        }
    }

    /// Terminal info (total_lines, cursor, display_offset, etc.).
    /// `absolute_line_count` and `clear_epoch` come from the session's state.
    pub fn terminal_info(&self, absolute_line_count: usize, clear_epoch: usize) -> TerminalInfo {
        let term = self.term.lock();
        let total_lines = term.total_lines();
        TerminalInfo {
            total_lines,
            absolute_line_count: absolute_line_count.max(total_lines),
            cursor_line: term.grid().cursor.point.line.0,
            last_content_line: crate::last_content_line(&term),
            num_lines: term.screen_lines(),
            num_cols: term.columns(),
            display_offset: term.grid().display_offset(),
            clear_epoch,
        }
    }

    /// Current terminal mode flags.
    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Whether alt-screen mode is active (vim/less/etc.).
    pub fn is_alt_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    // ── Resize / scroll ────────────────────────────────────────────

    /// Check whether the terminal grid needs resizing (without performing it).
    /// The caller should call `pty_resize` **before** `resize_grid` to ensure
    /// the PTY/process knows the new size before output arrives.
    pub fn needs_resize(&self, rows: u16, cols: u16) -> bool {
        let term = self.term.lock();
        term.columns() != cols as usize || term.screen_lines() != rows as usize
    }

    /// Actually resize the terminal grid. Should be called **after** `pty_resize`.
    pub fn resize_grid(&self, rows: u16, cols: u16) {
        self.term.lock().resize(TerminalSize {
            cols: cols as usize,
            lines: rows as usize,
        });
    }

    /// Scroll the scrollback by `delta` lines (no-op in alt-screen).
    pub fn scroll(&self, delta: i32) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    /// Scroll to the bottom of the scrollback.
    pub fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            term.scroll_display(Scroll::Bottom);
        }
    }

    /// Scroll to the top of the scrollback.
    pub fn scroll_to_top(&self) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            let total = term.total_lines() as i32;
            term.scroll_display(Scroll::Delta(total));
        }
    }

    // ── Selection ──────────────────────────────────────────────────

    /// Start a new selection at (row, col).
    pub fn start_selection(&self, row: f32, col: f32, sel: SelectionType) {
        let mut term = self.term.lock();
        let (point, side) = Self::point_and_side(&term, row, col);
        term.selection = Some(Selection::new(sel, point, side));
    }

    /// Update the existing selection end point (while dragging).
    pub fn update_selection(&self, row: f32, col: f32) {
        let mut term = self.term.lock();
        let (point, side) = Self::point_and_side(&term, row, col);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }

    /// Get the selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// Whether a non-empty selection exists (no text materialised — PERF-14).
    pub fn has_selection(&self) -> bool {
        let term = self.term.lock();
        term.selection
            .as_ref()
            .and_then(|selection| selection.to_range(&term))
            .is_some()
    }

    /// Clear the current selection.
    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    /// Select the entire scrollback.
    pub fn select_all(&self) {
        let mut term = self.term.lock();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
        sel.update(end, Side::Right);
        term.selection = Some(sel);
    }

    // ── Search ─────────────────────────────────────────────────────

    /// Search the terminal grid for `query`.
    ///
    /// The grid text is copied under the `Term` lock and matched after the lock
    /// is released, so a long scrollback search never stalls the pump (PERF-04).
    pub fn search(&self, query: &str, options: SearchOptions) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let text = {
            let term = self.term.lock();
            GridText::from_term(&*term)
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
        let mode = *term.mode();
        let display_offset = term.grid().display_offset();

        if display_offset > 0 {
            if !mode.contains(TermMode::ALT_SCREEN) {
                term.scroll_display(Scroll::Delta(scroll_delta));
            }
            None
        } else if mode.intersects(TermMode::MOUSE_MODE) {
            let bytes = encode_wheel_event(row as usize, col as usize, delta_y, mode, mods);
            Some(bytes)
        } else if mode.contains(TermMode::ALT_SCREEN) {
            let app_cursor = mode.contains(TermMode::APP_CURSOR);
            let key = match (delta_y > 0.0, app_cursor) {
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
            term.scroll_display(Scroll::Delta(scroll_delta));
            None
        }
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Convert pixel (row, col) → grid `Point` + `Side` for selection.
    pub fn point_and_side(term: &Term<EP>, row: f32, col: f32) -> (Point, Side) {
        let col = col.max(0.0);
        let row_idx = (row.max(0.0) as usize).min(term.screen_lines().saturating_sub(1));
        let column = (col as usize).min(term.columns().saturating_sub(1));
        let line = row_idx as i32 - term.grid().display_offset() as i32;
        let side = if col.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (Point::new(Line(line), Column(column)), side)
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::term::test::mock_term;

    use super::*;

    fn model(text: &str) -> TerminalModel<alacritty_terminal::event::VoidListener> {
        TerminalModel::new(Arc::new(FairMutex::new(mock_term(text))))
    }

    /// PERF-14: `has_selection` agrees with `selection_text` without building the string.
    #[test]
    fn has_selection_tracks_selection_state() {
        let model = model("hello world");
        assert!(!model.has_selection());

        model.start_selection(0.0, 0.0, SelectionType::Simple);
        model.update_selection(0.0, 4.9);
        assert!(model.has_selection());
        assert_eq!(model.selection_text().as_deref(), Some("hello"));

        model.clear_selection();
        assert!(!model.has_selection());
        assert!(model.selection_text().is_none());
    }

    #[test]
    fn select_all_marks_a_selection() {
        let model = model("one\ntwo");
        model.select_all();
        assert!(model.has_selection());
    }
}
