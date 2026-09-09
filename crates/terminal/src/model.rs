//! Shared terminal-model adapter — eliminates duplication between the local
//! and SSH backends.
//!
//! Both `LocalSession` and `SshSession` wrap an `alacritty_terminal::Term<EP>`
//! behind a `FairMutex` and implement the same terminal-model operations
//! (snapshot, query, scroll, selection, search, mouse encoding, etc.).
//! This module provides a single `TerminalModel<EP>` that both backends
//! delegate to, so the logic lives in one place.

use std::mem;
use std::sync::Arc;

use alacritty_terminal::grid::{Dimensions, Grid, Scroll};
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
use crate::osc_color::{BACKGROUND_INDEX, CURSOR_INDEX, DynamicColors, FOREGROUND_INDEX};
use crate::search::{GridText, search_grid_text};
use crate::{
    IndexedCell, LineRangeCells, SearchMatch, SearchOptions, TerminalInfo, TerminalQueryState,
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

/// How a grow-resize treats the primary grid's scrollback (DEC-0008).
///
/// Chosen by the backend that owns the PTY, because it must agree with whatever
/// sits on the other side of that PTY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizePolicy {
    /// alacritty's behaviour: added rows are pulled from scrollback into the
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

/// Shared terminal-model operations backed by an `alacritty_terminal::Term`.
///
/// Created once per session and stored inside the `LocalSession` / `SshSession`
/// struct. All methods are identical across backends — the only differences
/// (transport: PTY vs SSH channel, lifecycle, state fields, resize policy)
/// remain on the session structs themselves.
pub struct TerminalModel<EP: EventListener> {
    term: Arc<FairMutex<Term<EP>>>,
    resize_policy: ResizePolicy,
}

impl<EP: EventListener> TerminalModel<EP> {
    /// Wrap an existing `Arc<FairMutex<Term<EP>>>`.
    pub fn new(term: Arc<FairMutex<Term<EP>>>, resize_policy: ResizePolicy) -> Self {
        Self {
            term,
            resize_policy,
        }
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

    /// Snapshot the grid for rendering into a reusable buffer (consumes damage).
    /// Reuses `out`'s allocations — zero steady-state allocation per frame.
    pub fn snapshot_into(&self, out: &mut TerminalContent) {
        let mut term = self.term.lock();
        out.refill(&mut term);
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
    ///
    /// Shrinks and column-only changes always use alacritty's semantics; a grow
    /// follows the session's [`ResizePolicy`].
    pub fn resize_grid(&self, rows: u16, cols: u16) {
        let (lines, cols) = (rows as usize, cols as usize);
        let mut term = self.term.lock();
        let lines_added = lines.saturating_sub(term.screen_lines());
        if self.resize_policy == ResizePolicy::KeepViewportTop && lines_added > 0 {
            grow_keeping_viewport_top(&mut term, lines, cols, lines_added);
        } else {
            term.resize(TerminalSize { cols, lines });
        }
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

/// Grow the viewport by `lines_added` rows without pulling scrollback into it
/// ([`ResizePolicy::KeepViewportTop`]).
///
/// `Grid::grow_lines` (vendored `grid/resize.rs`) grows the ring buffer with
/// the bottom row anchored, which pulls `pulled = min(history_size,
/// lines_added)` history rows into the top of the viewport, moves the cursor
/// and saved cursor down by `pulled`, subtracts `lines_added` from
/// `display_offset` and drops `pulled` rows of history. This runs that resize
/// and then undoes the pull on the primary grid. Invariants relied on:
///
/// - `Grid::scroll_up` over the whole screen by `pulled` rotates the top
///   `pulled` rows back into history and clears the bottom `pulled` rows. Its
///   `increase_scroll_limit` always finds room for exactly `pulled` rows,
///   because the resize removed that many and the scroll limit never shrank,
///   so `history_size` returns to its pre-resize value and every pre-resize
///   row keeps its `Line` index; the ring buffer stays consistent because the
///   rotation only moves `zero` and the rows it exposes are reset.
/// - Both cursors moved down by exactly `pulled`, so moving them back up keeps
///   them inside the viewport.
/// - The pre-resize `display_offset` is restored (clamped to history): the top
///   visible row stays where it was and the viewport extends downward, for a
///   user scrolled back as much as for one at the bottom.
/// - Rows are resized before columns so the correction is exact; the column
///   reflow then runs on the corrected grid.
/// - `Term::resize` marks the whole terminal damaged and nothing resets damage
///   while the lock is held, so the corrected rows repaint on the next
///   snapshot. The selection is dropped: alacritty rotated it by `pulled`.
///
/// While the alt screen is active the primary grid is `Term::inactive_grid`,
/// which has no accessor. The alt grid (no history, so nothing to correct) is
/// parked in a local and replaced by a placeholder, `swap_alt` makes the primary
/// grid active for the resize and the correction, then `swap_alt` returns to the
/// alt screen — that direction clones the primary cursor into, and clears, the
/// placeholder only — and the parked alt grid, resized exactly as `Term::resize`
/// would have (no reflow), is put back. The two swaps restore the keyboard-mode
/// stacks and the mode flags; `Term::resize` runs with the same reflow flags as
/// before the swap.
fn grow_keeping_viewport_top<EP: EventListener>(
    term: &mut Term<EP>,
    lines: usize,
    cols: usize,
    lines_added: usize,
) {
    let old_cols = term.columns();
    let parked_alt = term.mode().contains(TermMode::ALT_SCREEN).then(|| {
        let placeholder = Grid::new(term.screen_lines(), old_cols, 0);
        let alt = mem::replace(term.grid_mut(), placeholder);
        term.swap_alt();
        alt
    });

    let pulled = term.history_size().min(lines_added);
    let display_offset = term.grid().display_offset();
    term.resize(TerminalSize {
        cols: old_cols,
        lines,
    });
    if pulled > 0 {
        let grid = term.grid_mut();
        grid.scroll_up(&(Line(0)..Line(lines as i32)), pulled);
        grid.cursor.point.line -= pulled;
        grid.saved_cursor.point.line -= pulled;
        let offset_delta = display_offset as i32 - grid.display_offset() as i32;
        grid.scroll_display(Scroll::Delta(offset_delta));
        term.selection = None;
    }
    if cols != old_cols {
        term.resize(TerminalSize { cols, lines });
    }

    if let Some(mut alt) = parked_alt {
        alt.resize(false, lines, cols);
        term.swap_alt();
        *term.grid_mut() = alt;
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::term::test::mock_term;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;

    fn model(text: &str) -> TerminalModel<VoidListener> {
        TerminalModel::new(
            Arc::new(FairMutex::new(mock_term(text))),
            ResizePolicy::Default,
        )
    }

    /// A 5x10 terminal whose primary screen was fed `lines` numbered lines and
    /// a prompt, so the cursor sits on the last row and the oldest lines are in
    /// scrollback. `bytes` are fed last.
    fn fed_term(lines: usize, bytes: &[u8]) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: 100,
            ..Default::default()
        };
        let mut term = Term::new(config, &TerminalSize { cols: 10, lines: 5 }, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        for i in 0..lines {
            parser.advance(&mut term, format!("line{i}\r\n").as_bytes());
        }
        parser.advance(&mut term, b"prompt>");
        parser.advance(&mut term, bytes);
        term
    }

    fn resize_model(term: Term<VoidListener>, policy: ResizePolicy) -> TerminalModel<VoidListener> {
        TerminalModel::new(Arc::new(FairMutex::new(term)), policy)
    }

    fn row_text(term: &Term<VoidListener>, line: i32) -> String {
        let row = &term.grid()[Line(line)];
        (0..term.columns())
            .map(|col| row[Column(col)].c)
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
        Processor::<StdSyncHandler>::new().advance(term, bytes);
    }

    #[test]
    fn keep_viewport_top_grow_keeps_rows_cursor_and_history() {
        let model = resize_model(fed_term(20, b""), ResizePolicy::KeepViewportTop);
        model.resize_grid(8, 10);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point.line, Line(4));
        assert_eq!(row_text(&term, 0), "line16");
        assert_eq!(row_text(&term, 4), "prompt>");
        for line in 5..8 {
            assert_eq!(row_text(&term, line), "", "row {line} must be blank");
        }
        assert_eq!(row_text(&term, -1), "line15");
        assert_eq!(term.history_size(), 16);
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn default_grow_pulls_history_and_moves_the_cursor_down() {
        let model = resize_model(fed_term(20, b""), ResizePolicy::Default);
        model.resize_grid(8, 10);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point.line, Line(7));
        assert_eq!(row_text(&term, 0), "line13");
        assert_eq!(row_text(&term, 7), "prompt>");
        assert_eq!(term.history_size(), 13);
    }

    #[test]
    fn keep_viewport_top_grow_larger_than_history() {
        // Two history lines, five rows added: only two could have been pulled.
        let model = resize_model(fed_term(6, b""), ResizePolicy::KeepViewportTop);
        model.resize_grid(10, 10);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point.line, Line(4));
        assert_eq!(row_text(&term, 0), "line2");
        assert_eq!(row_text(&term, -2), "line0");
        for line in 5..10 {
            assert_eq!(row_text(&term, line), "", "row {line} must be blank");
        }
        assert_eq!(term.history_size(), 2);
    }

    #[test]
    fn keep_viewport_top_repeated_grows_and_column_change() {
        let model = resize_model(fed_term(20, b""), ResizePolicy::KeepViewportTop);
        model.resize_grid(8, 12);
        model.resize_grid(12, 20);

        let term = model.term().lock();
        assert_eq!(term.columns(), 20);
        assert_eq!(term.grid().cursor.point.line, Line(4));
        assert_eq!(row_text(&term, 0), "line16");
        assert_eq!(row_text(&term, 4), "prompt>");
        assert_eq!(row_text(&term, 11), "");
        assert_eq!(term.history_size(), 16);
    }

    #[test]
    fn keep_viewport_top_restores_a_scrolled_back_viewport() {
        let model = resize_model(fed_term(20, b""), ResizePolicy::KeepViewportTop);
        model.term().lock().scroll_display(Scroll::Delta(3));
        model.resize_grid(8, 10);

        let term = model.term().lock();
        assert_eq!(term.grid().display_offset(), 3);
        assert_eq!(term.grid().cursor.point.line, Line(4));
    }

    #[test]
    fn keep_viewport_top_leaves_shrink_and_history_less_grow_to_alacritty() {
        for (rows, lines) in [(3, 20), (8, 2)] {
            let keep = resize_model(fed_term(lines, b""), ResizePolicy::KeepViewportTop);
            let default = resize_model(fed_term(lines, b""), ResizePolicy::Default);
            keep.resize_grid(rows, 10);
            default.resize_grid(rows, 10);

            let (keep, default) = (keep.term().lock(), default.term().lock());
            assert_eq!(keep.grid().cursor.point, default.grid().cursor.point);
            assert_eq!(keep.history_size(), default.history_size());
            for line in -(keep.history_size() as i32)..rows as i32 {
                assert_eq!(row_text(&keep, line), row_text(&default, line));
            }
        }
    }

    #[test]
    fn keep_viewport_top_grow_during_alt_screen_corrects_the_primary_grid() {
        let model = resize_model(
            fed_term(20, b"\x1b[?1049h\x1b[Htui"),
            ResizePolicy::KeepViewportTop,
        );
        model.resize_grid(8, 10);

        {
            let term = model.term().lock();
            assert!(term.mode().contains(TermMode::ALT_SCREEN));
            assert_eq!(row_text(&term, 0), "tui");
            assert_eq!(term.grid().cursor.point, Point::new(Line(0), Column(3)));
            assert_eq!(term.history_size(), 0);
        }

        let mut term = model.term().lock();
        feed(&mut term, b"\x1b[?1049l");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));
        assert_eq!(row_text(&term, 0), "line16");
        assert_eq!(row_text(&term, 4), "prompt>");
        for line in 5..8 {
            assert_eq!(row_text(&term, line), "", "row {line} must be blank");
        }
        assert_eq!(term.history_size(), 16);
    }

    #[test]
    fn default_grow_during_alt_screen_pulls_the_primary_history() {
        let model = resize_model(fed_term(20, b"\x1b[?1049h\x1b[Htui"), ResizePolicy::Default);
        model.resize_grid(8, 10);

        let mut term = model.term().lock();
        feed(&mut term, b"\x1b[?1049l");
        assert_eq!(term.grid().cursor.point.line, Line(7));
        assert_eq!(term.history_size(), 13);
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
