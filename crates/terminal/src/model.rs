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
use alacritty_terminal::term::cell::Cell;
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
    /// Follows the session's [`ResizePolicy`].
    pub fn resize_grid(&self, rows: u16, cols: u16) {
        let (lines, cols) = (rows as usize, cols as usize);
        let mut term = self.term.lock();
        if self.resize_policy == ResizePolicy::KeepViewportTop {
            resize_keeping_viewport_top(&mut term, lines, cols);
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

/// Resize the primary grid the way conhost resizes its buffer behind ConPTY
/// ([`ResizePolicy::KeepViewportTop`]).
///
/// Measured on Windows 11 (BUG-0051, raw PTY dumps): conhost repaints nothing
/// after `ResizePseudoConsole`. It re-wraps the rows of the old viewport at the
/// new width as if the top row started a line, keeps that content at the top,
/// leaves the rows below blank and addresses later output with absolute cursor
/// positions in those coordinates. alacritty anchors the bottom row instead:
/// `grow_lines` pulls history into the top, `grow_columns` lets history fill the
/// rows freed by joining wrapped rows, `shrink_columns` pushes the top rows out
/// when it splits rows. So this runs alacritty's resize, measures the row
/// conhost puts the cursor on ([`conhost_cursor_row`]) and shifts the viewport
/// by the difference:
///
/// - a positive shift is `Grid::scroll_up` over the whole screen: the top rows
///   rotate back into history (`increase_scroll_limit` makes room while below
///   the scroll limit; the ring stays consistent because only `zero` moves and
///   the exposed bottom rows are reset) and the cursors move up with the rows;
/// - a negative shift (a column shrink split rows above the cursor) grows the
///   rows by the shift, which pulls those rows back from history and moves the
///   cursors down, then shrinks the rows again, which drops the bottom rows
///   (blank: they were below the cursor);
/// - the pre-resize `display_offset` is restored (clamped to history), so a
///   scrolled-back viewport keeps its top row; the selection is dropped
///   (alacritty rotated it); `Term::resize` leaves the whole terminal damaged
///   and nothing resets damage while the lock is held.
///
/// Only the cursor row is aligned. When the top row continues a wrapped line
/// from history, conhost shows that line torn at the old top row while this
/// grid shows it re-wrapped whole, so the top rows can hold other text than
/// conhost's; every row that starts at column 0 below it agrees.
///
/// While the alt screen is active the primary grid is `Term::inactive_grid`,
/// which has no accessor. The alt grid (no history, nothing to correct) is
/// parked in a local and replaced by a placeholder, `swap_alt` makes the primary
/// grid active for the resize and the correction, then `swap_alt` returns to the
/// alt screen — that direction clones the primary cursor into, and clears, the
/// placeholder only — and the parked alt grid, resized exactly as `Term::resize`
/// would have (no reflow), is put back. The two swaps restore the keyboard-mode
/// stacks and the mode flags; `Term::resize` runs with the same reflow flags as
/// before the swap.
fn resize_keeping_viewport_top<EP: EventListener>(term: &mut Term<EP>, lines: usize, cols: usize) {
    let parked_alt = term.mode().contains(TermMode::ALT_SCREEN).then(|| {
        let placeholder = Grid::new(term.screen_lines(), term.columns(), 0);
        let alt = mem::replace(term.grid_mut(), placeholder);
        term.swap_alt();
        alt
    });

    let display_offset = term.grid().display_offset();
    let conhost_row = conhost_cursor_row(term.grid(), cols).min(lines - 1) as i32;
    term.resize(TerminalSize { cols, lines });

    let grid = term.grid_mut();
    let shift = grid.cursor.point.line.0 - conhost_row;
    if shift > 0 {
        let shift = shift as usize;
        grid.scroll_up(&(Line(0)..Line(lines as i32)), shift);
        grid.cursor.point.line -= shift;
        grid.saved_cursor.point.line = Line((grid.saved_cursor.point.line.0 - shift as i32).max(0));
    } else if shift < 0 {
        let pull = (shift.unsigned_abs() as usize).min(grid.history_size());
        grid.resize(true, lines + pull, cols);
        grid.resize(true, lines, cols);
    }
    if shift != 0 {
        let offset_delta = display_offset as i32 - grid.display_offset() as i32;
        grid.scroll_display(Scroll::Delta(offset_delta));
        term.selection = None;
    }

    if let Some(mut alt) = parked_alt {
        alt.resize(false, lines, cols);
        term.swap_alt();
        *term.grid_mut() = alt;
    }
}

/// The viewport row conhost puts the cursor on after re-wrapping the viewport
/// to `cols` columns (measured behaviour, see [`resize_keeping_viewport_top`]).
///
/// The rows from the top of the viewport down to the cursor row are copied into
/// a history-less scratch grid and reflowed there, so the vendored reflow
/// decides which rows join or split exactly as it will for the real grid, and
/// the cursor's distance from the top row is read back as its buffer row
/// (`history_size` + line: a shrink pushes split rows into the scratch history).
/// `pad` blank rows above the copy keep the scratch cursor off `Line(0)`, where
/// `shrink_columns` clamps it instead of letting rows split below it.
fn conhost_cursor_row(grid: &Grid<Cell>, cols: usize) -> usize {
    let old_cols = grid.columns();
    let cursor = &grid.cursor;
    let pad = old_cols / cols + 1;
    let rows = pad + cursor.point.line.0 as usize + 1;
    let mut probe: Grid<Cell> = Grid::new(rows, old_cols, rows * old_cols);
    for line in 0..=cursor.point.line.0 {
        probe[Line(pad as i32 + line)] = grid[Line(line)].clone();
    }
    probe.cursor.point = Point::new(Line(rows as i32 - 1), cursor.point.column);
    probe.cursor.input_needs_wrap = cursor.input_needs_wrap;
    probe.resize(true, rows, cols);
    probe.history_size() + probe.cursor.point.line.0 as usize - pad
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::term::cell::Flags;
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

    /// A 5x10 terminal fed `rows` (each followed by CR LF) and then `prompt`.
    fn term_with(rows: &[&str], prompt: &str) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: 100,
            ..Default::default()
        };
        let mut term = Term::new(config, &TerminalSize { cols: 10, lines: 5 }, VoidListener);
        let mut parser = Processor::<StdSyncHandler>::new();
        for row in rows {
            parser.advance(&mut term, format!("{row}\r\n").as_bytes());
        }
        parser.advance(&mut term, prompt.as_bytes());
        term
    }

    fn wraps(term: &Term<VoidListener>, line: i32) -> bool {
        term.grid()[Line(line)][Column(term.columns() - 1)]
            .flags
            .contains(Flags::WRAPLINE)
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

    /// `ls -lath` case (BUG-0051 rework): a wrapped line inside the viewport
    /// joins on a widen; conhost keeps the top row and moves the cursor up by
    /// the rows that vanished, so must the grid. The joined rows return to
    /// history instead of being replaced by pulled history.
    #[test]
    fn keep_viewport_top_widen_joins_wrapped_rows_and_keeps_the_top_row() {
        let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.push("abcdefghijklmnopqrstuvwxyz");
        let term = term_with(&rows, "prompt>");
        assert_eq!(row_text(&term, 0), "line9");
        assert!(wraps(&term, 1) && wraps(&term, 2) && !wraps(&term, 3));
        assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));

        let model = resize_model(term, ResizePolicy::KeepViewportTop);
        model.resize_grid(8, 30);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(7)));
        assert_eq!(row_text(&term, 0), "line9");
        assert_eq!(row_text(&term, 1), "abcdefghijklmnopqrstuvwxyz");
        assert!(!wraps(&term, 1));
        assert_eq!(row_text(&term, 2), "prompt>");
        for line in 3..8 {
            assert_eq!(row_text(&term, line), "", "row {line} must be blank");
        }
        assert_eq!(row_text(&term, -1), "line8");
        assert_eq!(term.history_size(), 9);
    }

    /// A pure widen (rows unchanged) needs the same correction: conhost moves
    /// the cursor up by the joined rows while alacritty keeps its index.
    #[test]
    fn keep_viewport_top_widen_without_row_change_moves_the_cursor_up() {
        let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.push("abcdefghijklmnopqrstuvwxyz");
        let model = resize_model(term_with(&rows, "prompt>"), ResizePolicy::KeepViewportTop);
        model.resize_grid(5, 30);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(7)));
        assert_eq!(row_text(&term, 0), "line9");
        assert_eq!(row_text(&term, 1), "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(row_text(&term, 2), "prompt>");
        assert_eq!(row_text(&term, 3), "");
        assert_eq!(row_text(&term, 4), "");
        assert_eq!(term.history_size(), 9);
    }

    /// The top row is the tail of a line wrapped from history. conhost re-wraps
    /// the viewport from its top row, so nothing above the cursor changes and
    /// the cursor row stays; alacritty joins the tail into the history line,
    /// which then shows whole on the top row.
    #[test]
    fn keep_viewport_top_top_row_continuing_a_history_line_keeps_the_cursor_row() {
        let rows: Vec<String> = (0..8).map(|i| format!("line{i}")).collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.extend(["abcdefghijklmnopqrstuvwxyz", "a", "b", "c"]);
        let term = term_with(&rows, "prompt>");
        assert_eq!(row_text(&term, 0), "uvwxyz");
        assert!(wraps(&term, -1) && !wraps(&term, 0));

        let model = resize_model(term, ResizePolicy::KeepViewportTop);
        model.resize_grid(8, 30);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(7)));
        assert_eq!(row_text(&term, 0), "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(row_text(&term, 1), "a");
        assert_eq!(row_text(&term, 3), "c");
        assert_eq!(row_text(&term, 4), "prompt>");
        assert_eq!(row_text(&term, 5), "");
        assert_eq!(row_text(&term, -1), "line7");
        assert_eq!(term.history_size(), 8);
    }

    /// The cursor sits on the continuation row of a wrapped command line: the
    /// join moves it up one row and right by the joined width, as in conhost.
    #[test]
    fn keep_viewport_top_widen_joins_the_cursor_row() {
        let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let term = term_with(&rows, "prompt>abcdef");
        assert!(wraps(&term, 3));
        assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(3)));

        let model = resize_model(term, ResizePolicy::KeepViewportTop);
        model.resize_grid(5, 30);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point, Point::new(Line(3), Column(13)));
        assert_eq!(row_text(&term, 0), "line7");
        assert_eq!(row_text(&term, 3), "prompt>abcdef");
        assert_eq!(row_text(&term, 4), "");
        assert_eq!(term.history_size(), 7);
    }

    /// A column shrink splits a row above a mid-screen cursor: conhost keeps
    /// the top row and the cursor moves down; alacritty pushed the split row
    /// into history, so the grid pulls it back and drops a blank bottom row.
    #[test]
    fn keep_viewport_top_narrow_with_a_mid_screen_cursor_pulls_split_rows_back() {
        let term = term_with(&["abcdefgh"], "p>");
        assert_eq!(term.grid().cursor.point, Point::new(Line(1), Column(2)));

        let model = resize_model(term, ResizePolicy::KeepViewportTop);
        model.resize_grid(5, 6);

        let term = model.term().lock();
        assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(2)));
        assert_eq!(row_text(&term, 0), "abcdef");
        assert!(wraps(&term, 0));
        assert_eq!(row_text(&term, 1), "gh");
        assert_eq!(row_text(&term, 2), "p>");
        assert_eq!(row_text(&term, 3), "");
        assert_eq!(row_text(&term, 4), "");
        assert_eq!(term.history_size(), 0);
    }

    /// A column shrink with the cursor on the bottom row: the split row pushes
    /// the top row out on both sides, so the default result already matches.
    #[test]
    fn keep_viewport_top_narrow_with_the_cursor_at_the_bottom_matches_alacritty() {
        let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.push("abcdefgh");
        let keep = resize_model(term_with(&rows, "p>"), ResizePolicy::KeepViewportTop);
        let default = resize_model(term_with(&rows, "p>"), ResizePolicy::Default);
        keep.resize_grid(5, 6);
        default.resize_grid(5, 6);

        let (keep, default) = (keep.term().lock(), default.term().lock());
        assert_eq!(keep.grid().cursor.point, Point::new(Line(4), Column(2)));
        assert_eq!(keep.grid().cursor.point, default.grid().cursor.point);
        assert_eq!(keep.history_size(), default.history_size());
        for line in -(keep.history_size() as i32)..5 {
            assert_eq!(row_text(&keep, line), row_text(&default, line));
        }
    }

    /// Wrapped rows on the primary screen join while a TUI holds the alt
    /// screen: the correction runs on the inactive primary grid and the prompt
    /// lands on the joined row after the TUI exits.
    #[test]
    fn keep_viewport_top_widen_during_alt_screen_joins_the_primary_rows() {
        let rows: Vec<String> = (0..10).map(|i| format!("line{i}")).collect();
        let mut rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        rows.push("abcdefghijklmnopqrstuvwxyz");
        let mut term = term_with(&rows, "prompt>tui\r\n");
        assert_eq!(term.grid().cursor.point, Point::new(Line(4), Column(0)));
        feed(&mut term, b"\x1b[?1049h\x1b[Htui");

        let model = resize_model(term, ResizePolicy::KeepViewportTop);
        model.resize_grid(8, 30);

        let mut term = model.term().lock();
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        feed(&mut term, b"\x1b[?1049l");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(term.grid().cursor.point, Point::new(Line(2), Column(0)));
        assert_eq!(row_text(&term, 0), "abcdefghijklmnopqrstuvwxyz");
        assert_eq!(row_text(&term, 1), "prompt>tui");
        for line in 2..8 {
            assert_eq!(row_text(&term, line), "", "row {line} must be blank");
        }
        assert_eq!(term.history_size(), 10);
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
