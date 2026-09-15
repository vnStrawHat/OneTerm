//! Deterministic terminal fakes for cross-crate tests and diagnostic harnesses.
//!
//! Both shapes here sit on a **real** `oneterm-vt` terminal. `US-0085` deleted
//! the hand-fabricated `TerminalContent`: the render state has no public
//! constructor for rows, and building one would have meant an engine API whose
//! only caller is a downstream test. Writing the cells into a real grid instead
//! costs less code and makes every frame a frame the engine actually produced.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_channel::{Receiver, Sender, TrySendError};
use oneterm_vt::MouseReporting;
use oneterm_vt::{
    Cell, CellContent, CellWidth, CursorShape, EventBatch, Extras, ModeSnapshot, Pos, Rgb,
    SelectionKind, Side, Size, Style, Terminal,
};

use crate::content::{LineRangeCells, TerminalContent};
use crate::osc_color::DynamicColors;
use crate::session::{
    SessionEvent, SessionKind, TerminalError, TerminalIme, TerminalInfo, TerminalInput,
    TerminalLifecycle, TerminalQueryState, TerminalRender, TerminalSession,
};
use oneterm_vt::input::{MouseModifiers, TerminalMouseButton};
use oneterm_vt::search::{SearchMatch, SearchOptions};

// ─────────────────────────── the grid fixture ───────────────────────────

/// One cell as a test wants to state it, before it is interned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureCell {
    /// The base scalar.
    pub ch: char,
    /// Combining marks / ZWJ followers attached to `ch`.
    pub zerowidth: Vec<char>,
    pub style: Style,
    pub width: CellWidth,
    /// `(id, uri)` of an OSC 8 link; an empty `id` means the stream gave none.
    pub hyperlink: Option<(String, String)>,
}

impl Default for FixtureCell {
    fn default() -> FixtureCell {
        FixtureCell {
            ch: ' ',
            zerowidth: Vec::new(),
            style: Style::DEFAULT,
            width: CellWidth::Narrow,
            hyperlink: None,
        }
    }
}

/// A terminal whose grid is written cell by cell.
///
/// For tests that need a frame of a specific shape — a wide pair without a wide
/// character, a `WRAPLINE` without a wrap, one styled cell in the middle of a
/// row — which an escape-sequence stream cannot state directly. Everything else
/// (cursor, modes, images, scrollback) goes through [`GridFixture::feed`],
/// because the engine is right there.
pub struct GridFixture {
    term: Terminal,
    rows: u16,
    cols: u16,
}

impl GridFixture {
    /// A fresh terminal at `rows` x `cols` with the default scrollback.
    pub fn new(rows: usize, cols: usize) -> GridFixture {
        let (rows, cols) = (rows.max(1) as u16, cols.max(1) as u16);
        GridFixture {
            term: Terminal::new(
                Size { rows, cols },
                oneterm_vt::Config {
                    scrollback_limit: crate::handle::DEFAULT_SCROLLBACK_LINES as u32,
                    ..oneterm_vt::Config::default()
                },
            ),
            rows,
            cols,
        }
    }

    /// The engine, for anything the helpers below do not cover.
    pub fn terminal(&mut self) -> &mut Terminal {
        &mut self.term
    }

    /// Feed bytes and drop the events.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut batch = EventBatch::new();
        self.term.feed(bytes, &mut batch, Instant::now());
    }

    /// Open a new batch, so the rows written after it carry a sequence number a
    /// render state has not seen — which is what makes a refill copy them.
    pub fn begin_batch(&mut self) {
        self.term.grid_mut().begin_batch();
    }

    /// Write one cell at `(row, col)` of the active screen.
    pub fn write(&mut self, row: usize, col: usize, cell: &FixtureCell) {
        if row >= usize::from(self.rows) || col >= usize::from(self.cols) {
            return;
        }
        // Two steps: the interner and the grid are disjoint fields of the
        // engine, but `Terminal` hands them out one borrow at a time.
        let interner = self.term.interner_mut();
        let style = interner.style(&cell.style);
        let content = if cell.zerowidth.is_empty() {
            CellContent::Scalar(cell.ch)
        } else {
            let mut cluster = Vec::with_capacity(cell.zerowidth.len() + 1);
            cluster.push(cell.ch);
            cluster.extend_from_slice(&cell.zerowidth);
            CellContent::Grapheme(interner.grapheme(&cluster))
        };
        let extras = match &cell.hyperlink {
            None => oneterm_vt::ExtrasId::NONE,
            Some((id, uri)) => {
                let id = (!id.is_empty()).then_some(id.as_str());
                let hyperlink = interner.hyperlinks.intern(id, uri);
                interner.extras(&Extras {
                    hyperlink,
                    graphic: None,
                })
            }
        };
        let packed = Cell::EMPTY
            .with_content(content)
            .with_style(style)
            .with_width(cell.width)
            .with_extras(extras);
        let id = self.term.screen().screen_top() + row as u64;
        // `set` clears the row's wrap flag, so a caller that wants one calls
        // `set_wrapped` after its writes — exactly as the engine's print path
        // does.
        self.term
            .grid_mut()
            .screen_mut()
            .row_mut(id)
            .set(col as u16, packed);
    }

    /// Mark (or unmark) `row` as continuing on the next one.
    pub fn set_wrapped(&mut self, row: usize, wrapped: bool) {
        if row >= usize::from(self.rows) {
            return;
        }
        let id = self.term.screen().screen_top() + row as u64;
        self.term
            .grid_mut()
            .screen_mut()
            .row_mut(id)
            .set_wrapped(wrapped);
    }

    /// Push `rows` blank lines into history, so a later
    /// [`scroll_back`](Self::scroll_back) has somewhere to go. Call before
    /// writing cells: it moves the screen top.
    pub fn grow_history(&mut self, rows: usize) {
        for _ in 0..rows {
            self.feed(b"\r\n");
        }
    }

    /// Move the viewport `rows` rows towards history (clamped by the history
    /// that exists).
    pub fn scroll_back(&mut self, rows: usize) {
        let grid = self.term.grid_mut();
        grid.screen_mut().scroll_viewport(-(rows as i32));
        grid.sync_anchors();
    }

    /// Move the viewport `rows` rows back towards the bottom.
    pub fn scroll_forward(&mut self, rows: usize) {
        let grid = self.term.grid_mut();
        grid.screen_mut().scroll_viewport(rows as i32);
        grid.sync_anchors();
    }

    /// Select from `start` to `end`, both in `(screen row, column)`.
    pub fn select(&mut self, start: (usize, usize), end: (usize, usize), kind: SelectionKind) {
        let top = self.term.screen().screen_top();
        let pos = |(row, col): (usize, usize)| Pos {
            row: top + row as u64,
            col: col as u16,
        };
        self.term.selection_start(pos(start), Side::Left, kind);
        self.term.selection_update(pos(end), Side::Right);
    }

    /// Place the cursor at `(screen row, column)` and give it `shape`.
    pub fn cursor(&mut self, row: usize, col: usize, shape: CursorShape) {
        self.feed(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
        match shape {
            CursorShape::Hidden => self.feed(b"\x1b[?25l"),
            CursorShape::Block | CursorShape::HollowBlock => self.feed(b"\x1b[2 q"),
            CursorShape::Underline => self.feed(b"\x1b[4 q"),
            CursorShape::Beam => self.feed(b"\x1b[6 q"),
        }
    }

    /// The damage-free line-range read, as a session would answer it.
    pub fn line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        crate::model::line_range_cells(&self.term, start_line, count)
    }

    /// Refill a caller-owned frame buffer.
    pub fn refill(&mut self, content: &mut TerminalContent) {
        content.refill(&mut self.term);
    }

    /// One frame, with a fresh watermark (so it reports `Full`).
    pub fn snapshot(&mut self) -> TerminalContent {
        TerminalContent::from(&mut self.term)
    }
}

// ─────────────────────────── the fake session ───────────────────────────

/// One [`TerminalInput`] call captured by [`FakeSessionProbe`].
///
/// Byte writes stay in `writes()`; this records the calls that carry no bytes
/// of their own (mouse, wheel, viewport, selection), so a UI test can assert
/// what actually reached the session.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeInputCall {
    MouseDown {
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        selection: SelectionKind,
        mods: MouseModifiers,
    },
    MouseMove {
        row: f32,
        col: f32,
        mods: MouseModifiers,
    },
    MouseDrag {
        row: f32,
        col: f32,
        mods: MouseModifiers,
    },
    MouseUp {
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        mods: MouseModifiers,
    },
    Wheel {
        delta_y: f64,
        row: f32,
        col: f32,
        mods: MouseModifiers,
    },
    Scroll(i32),
    ScrollToTop,
    ScrollToBottom,
    ClearSelection,
    SelectAll,
    Clear,
}

/// Shared observation and control handle for a [`FakeTerminalSession`].
#[derive(Clone)]
pub struct FakeSessionProbe {
    state: Arc<FakeSessionState>,
}

impl FakeSessionProbe {
    /// Replace the text shown by the terminal.
    ///
    /// Only the rows that actually differ are repainted — CUP, the line, then
    /// `EL` — because that is what a program does, and because clearing the
    /// whole screen would stamp every row and make "only the changed rows are
    /// rebuilt" untestable.
    pub fn set_text(&self, text: impl Into<String>) {
        let text = text.into();
        let mut shown = self.state.shown.lock().unwrap();
        let new: Vec<&str> = text.lines().collect();
        let old: Vec<&str> = shown.lines().collect();
        let mut engine = self.state.engine.lock().unwrap();
        for row in 0..new.len().max(old.len()) {
            let (before, after) = (old.get(row).copied(), new.get(row).copied());
            if before == after {
                continue;
            }
            engine.feed(format!("\x1b[{};1H", row + 1).as_bytes());
            engine.feed(after.unwrap_or("").as_bytes());
            engine.feed(b"\x1b[K");
        }
        engine.feed(b"\x1b[H");
        drop(engine);
        *shown = text;
    }

    /// Feed raw bytes to the terminal behind the fake.
    pub fn feed(&self, bytes: &[u8]) {
        self.state.engine.lock().unwrap().feed(bytes);
    }

    /// Turn mouse reporting on (`? 1000` / `? 1002` / `? 1003`) or off.
    pub fn set_mouse_reporting(&self, reporting: Option<MouseReporting>) {
        let bytes: &[u8] = match reporting {
            None => b"\x1b[?1000l\x1b[?1002l\x1b[?1003l",
            Some(MouseReporting::Normal) => b"\x1b[?1000h",
            Some(MouseReporting::ButtonEvent) => b"\x1b[?1002h",
            Some(MouseReporting::AnyEvent) => b"\x1b[?1003h",
        };
        self.feed(bytes);
    }

    /// Enter or leave the alternate screen (a TUI took over / gave back).
    pub fn set_alt_screen(&self, on: bool) {
        self.feed(if on { b"\x1b[?1049h" } else { b"\x1b[?1049l" });
    }

    /// Set the cursor position (screen row, column).
    pub fn set_cursor(&self, row: usize, col: usize) {
        self.feed(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
    }

    /// Send an event to the session subscriber.
    pub fn emit(&self, event: SessionEvent) -> Result<(), TrySendError<SessionEvent>> {
        self.state.event_tx.try_send(event)
    }

    /// Return every write captured by the fake transport.
    pub fn writes(&self) -> Vec<Vec<u8>> {
        self.state.writes.lock().unwrap().clone()
    }

    /// Make every following `write` fail with `TerminalError::QueueFull`.
    pub fn fail_writes(&self, fail: bool) {
        self.state.fail_writes.store(fail, Ordering::SeqCst);
    }

    /// Remove and return all captured writes.
    pub fn take_writes(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.state.writes.lock().unwrap())
    }

    /// Return the number of render snapshots requested.
    pub fn snapshot_calls(&self) -> usize {
        self.state.snapshot_calls.load(Ordering::SeqCst)
    }

    /// Return the number of close requests received by the fake.
    pub fn close_calls(&self) -> usize {
        self.state.close_calls.load(Ordering::SeqCst)
    }

    /// Set whether the fake session accepts outbound input.
    pub fn set_alive(&self, alive: bool) {
        self.state.alive.store(alive, Ordering::SeqCst);
    }

    /// Return whether the fake session is alive.
    pub fn alive(&self) -> bool {
        self.state.alive.load(Ordering::SeqCst)
    }

    /// Return every non-byte input call captured so far, in order.
    pub fn input_calls(&self) -> Vec<FakeInputCall> {
        self.state.input_calls.lock().unwrap().clone()
    }

    /// Remove and return all captured input calls.
    pub fn take_input_calls(&self) -> Vec<FakeInputCall> {
        std::mem::take(&mut *self.state.input_calls.lock().unwrap())
    }

    /// Set the text `selection_text()` returns; `has_selection()` follows it.
    pub fn set_selection(&self, text: Option<String>) {
        *self.state.selection.lock().unwrap() = text;
    }

    /// Decode a Sixel image at the cursor, as a program would.
    ///
    /// The default payload is a two-band, two-column red block: 2 x 12 pixels,
    /// which is one virtual 10 x 20 cell wide and one tall.
    pub fn push_sixel(&self, payload: Option<&[u8]>) {
        let default: &[u8] = b"\x1bPq#0;2;100;0;0~~$-~~\x1b\\";
        self.feed(payload.unwrap_or(default));
    }

    /// Clear the screen, which releases every image on it.
    pub fn clear_graphics(&self) {
        self.feed(b"\x1b[H\x1b[2J");
    }
}

/// A deterministic in-memory implementation of [`TerminalSession`].
pub struct FakeTerminalSession {
    state: Arc<FakeSessionState>,
    event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    kind: SessionKind,
}

struct FakeSessionState {
    engine: Mutex<GridFixture>,
    /// What `set_text` last painted, so the next call repaints only the rows
    /// that differ.
    shown: Mutex<String>,
    writes: Mutex<Vec<Vec<u8>>>,
    input_calls: Mutex<Vec<FakeInputCall>>,
    selection: Mutex<Option<String>>,
    event_tx: Sender<SessionEvent>,
    fail_writes: AtomicBool,
    alive: AtomicBool,
    snapshot_calls: AtomicUsize,
    close_calls: AtomicUsize,
}

impl FakeTerminalSession {
    /// Create a fake session and its observation probe.
    pub fn new(rows: usize, cols: usize, text: impl Into<String>) -> (Self, FakeSessionProbe) {
        Self::new_with_kind(rows, cols, text, SessionKind::Local)
    }

    /// Create a fake session with an explicit backend kind.
    pub fn new_with_kind(
        rows: usize,
        cols: usize,
        text: impl Into<String>,
        kind: SessionKind,
    ) -> (Self, FakeSessionProbe) {
        let (event_tx, event_rx) = async_channel::bounded(64);
        let state = Arc::new(FakeSessionState {
            engine: Mutex::new(GridFixture::new(rows, cols)),
            shown: Mutex::new(String::new()),
            writes: Mutex::new(Vec::new()),
            input_calls: Mutex::new(Vec::new()),
            selection: Mutex::new(None),
            event_tx,
            fail_writes: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            snapshot_calls: AtomicUsize::new(0),
            close_calls: AtomicUsize::new(0),
        });
        let probe = FakeSessionProbe {
            state: state.clone(),
        };
        probe.set_text(text);
        (
            Self {
                state,
                event_rx: Mutex::new(Some(event_rx)),
                kind,
            },
            probe,
        )
    }

    /// Create a boxed fake session suitable for GPUI session entities.
    pub fn boxed(
        rows: usize,
        cols: usize,
        text: impl Into<String>,
    ) -> (Box<dyn TerminalSession>, FakeSessionProbe) {
        Self::boxed_with_kind(rows, cols, text, SessionKind::Local)
    }

    /// Create a boxed fake session with an explicit backend kind.
    pub fn boxed_with_kind(
        rows: usize,
        cols: usize,
        text: impl Into<String>,
        kind: SessionKind,
    ) -> (Box<dyn TerminalSession>, FakeSessionProbe) {
        let (session, probe) = Self::new_with_kind(rows, cols, text, kind);
        (Box::new(session), probe)
    }

    /// Append one observed [`TerminalInput`] call.
    fn record(&self, call: FakeInputCall) {
        self.state.input_calls.lock().unwrap().push(call);
    }

    fn modes(&self) -> ModeSnapshot {
        self.state.engine.lock().unwrap().term.mode_snapshot()
    }
}

impl TerminalSession for FakeTerminalSession {}

impl TerminalRender for FakeTerminalSession {
    fn snapshot(&self) -> TerminalContent {
        self.state.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        self.state.engine.lock().unwrap().snapshot()
    }

    fn snapshot_into(&self, out: &mut TerminalContent) {
        self.state.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        self.state.engine.lock().unwrap().refill(out);
    }

    fn query_state(&self) -> TerminalQueryState {
        let engine = self.state.engine.lock().unwrap();
        let term = &engine.term;
        let screen = term.screen();
        TerminalQueryState {
            modes: term.mode_snapshot(),
            cursor_row: screen.cursor().pos.row.distance(screen.screen_top()) as usize,
            cursor_col: usize::from(screen.cursor().pos.col),
            cursor_shape: term.cursor_style().shape,
            display_offset: screen.scroll_offset() as usize,
            rows: usize::from(screen.rows()),
            cols: usize::from(screen.cols()),
            total_lines: screen.history_len() as usize + usize::from(screen.rows()),
            alive: self.state.alive.load(Ordering::SeqCst),
        }
    }

    fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        crate::model::line_range_cells(&self.state.engine.lock().unwrap().term, start_line, count)
    }

    fn terminal_info(&self) -> TerminalInfo {
        let engine = self.state.engine.lock().unwrap();
        let term = &engine.term;
        let screen = term.screen();
        let total_lines = screen.history_len() as usize + usize::from(screen.rows());
        TerminalInfo {
            total_lines,
            absolute_line_count: total_lines,
            screen_top: screen.screen_top(),
            cursor_row: screen.cursor().pos.row.distance(screen.screen_top()) as usize,
            last_content_row: crate::last_content_row(term),
            num_lines: usize::from(screen.rows()),
            num_cols: usize::from(screen.cols()),
            display_offset: screen.scroll_offset() as usize,
            clear_epoch: 0,
        }
    }

    fn is_alt_screen(&self) -> bool {
        self.modes().alt_screen
    }

    fn is_mouse_mode(&self) -> bool {
        self.modes().mouse.is_some()
    }

    fn dynamic_colors(&self) -> DynamicColors {
        DynamicColors::default()
    }

    fn set_default_colors(
        &self,
        _foreground: Rgb,
        _background: Rgb,
        _cursor: Rgb,
        _ansi: [Rgb; 16],
    ) {
    }

    fn search(&self, _query: &str, _options: SearchOptions) -> Vec<SearchMatch> {
        Vec::new()
    }

    fn selection_text(&self) -> Option<String> {
        self.state.selection.lock().unwrap().clone()
    }

    fn has_selection(&self) -> bool {
        self.state
            .selection
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|t| !t.is_empty())
    }
}

impl TerminalInput for FakeTerminalSession {
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        if !self.state.alive.load(Ordering::SeqCst) {
            return Err(TerminalError::Closed);
        }
        if self.state.fail_writes.load(Ordering::SeqCst) {
            return Err(TerminalError::QueueFull);
        }
        self.state.writes.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }

    fn flush_pty(&self) {}

    fn send_ctrl_c(&self) {
        let _ = self.write(b"\x03");
    }

    fn resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        let mut engine = self.state.engine.lock().unwrap();
        let size = Size {
            rows: rows.max(1),
            cols: cols.max(1),
        };
        engine.rows = size.rows;
        engine.cols = size.cols;
        engine
            .term
            .resize(size, oneterm_vt::ResizePolicy::BottomAnchor);
        Ok(())
    }

    fn scroll(&self, delta: i32) {
        self.record(FakeInputCall::Scroll(delta));
    }

    fn scroll_to_bottom(&self) {
        self.record(FakeInputCall::ScrollToBottom);
    }

    fn scroll_to_top(&self) {
        self.record(FakeInputCall::ScrollToTop);
    }

    fn mouse_down(
        &self,
        row: f32,
        col: f32,
        button: TerminalMouseButton,
        kind: SelectionKind,
        mods: MouseModifiers,
    ) {
        self.record(FakeInputCall::MouseDown {
            row,
            col,
            button,
            selection: kind,
            mods,
        });
    }

    fn mouse_move(&self, row: f32, col: f32, mods: MouseModifiers) {
        self.record(FakeInputCall::MouseMove { row, col, mods });
    }

    fn mouse_drag(&self, row: f32, col: f32, mods: MouseModifiers) {
        self.record(FakeInputCall::MouseDrag { row, col, mods });
    }

    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton, mods: MouseModifiers) {
        self.record(FakeInputCall::MouseUp {
            row,
            col,
            button,
            mods,
        });
    }

    fn wheel(&self, delta_y: f64, row: f32, col: f32, mods: MouseModifiers) {
        self.record(FakeInputCall::Wheel {
            delta_y,
            row,
            col,
            mods,
        });
    }

    fn clear_selection(&self) {
        *self.state.selection.lock().unwrap() = None;
        self.record(FakeInputCall::ClearSelection);
    }

    fn select_all(&self) {
        self.record(FakeInputCall::SelectAll);
    }

    fn clear(&self) {
        self.record(FakeInputCall::Clear);
    }
}

impl TerminalIme for FakeTerminalSession {
    fn set_marked_text(&self, _text: String) {}

    fn clear_marked_text(&self) {}

    fn commit_text(&self, text: &str) {
        let _ = self.write(text.as_bytes());
    }

    fn marked_text(&self) -> Option<String> {
        None
    }
}

impl TerminalLifecycle for FakeTerminalSession {
    fn take_events(&self) -> Option<Receiver<SessionEvent>> {
        self.event_rx.lock().unwrap().take()
    }

    fn alive(&self) -> bool {
        self.state.alive.load(Ordering::SeqCst)
    }

    fn close(&self) -> Result<(), TerminalError> {
        self.state.close_calls.fetch_add(1, Ordering::SeqCst);
        self.state.alive.store(false, Ordering::SeqCst);
        let _ = self.state.event_tx.try_send(SessionEvent::Closed);
        Ok(())
    }

    fn kind(&self) -> SessionKind {
        self.kind
    }

    fn title(&self) -> Option<String> {
        None
    }

    fn cwd(&self) -> Option<PathBuf> {
        None
    }
}

/// In-memory [`PtyTransport`](crate::backend::PtyTransport): records writes,
/// resizes and close requests so pump/router tests need no PTY or network.
#[derive(Clone, Default)]
pub struct FakePtyTransport {
    inner: Arc<FakePtyTransportState>,
}

#[derive(Default)]
struct FakePtyTransportState {
    writes: Mutex<Vec<Vec<u8>>>,
    resizes: Mutex<Vec<(u16, u16)>>,
    closed: AtomicBool,
    fail_writes: AtomicBool,
}

impl FakePtyTransport {
    /// Create an empty transport.
    pub fn new() -> Self {
        Self::default()
    }

    /// Every write in order.
    pub fn writes(&self) -> Vec<Vec<u8>> {
        self.inner.writes.lock().unwrap().clone()
    }

    /// Remove and return every write.
    pub fn take_writes(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.inner.writes.lock().unwrap())
    }

    /// Every resize `(rows, cols)` in order.
    pub fn resizes(&self) -> Vec<(u16, u16)> {
        self.inner.resizes.lock().unwrap().clone()
    }

    /// Whether `pty_close` was called.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    /// Make every following write fail with `TerminalError::QueueFull`.
    pub fn fail_writes(&self, fail: bool) {
        self.inner.fail_writes.store(fail, Ordering::SeqCst);
    }
}

impl crate::backend::PtyTransport for FakePtyTransport {
    fn pty_write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
        if self.inner.fail_writes.load(Ordering::SeqCst) {
            return Err(TerminalError::QueueFull);
        }
        self.inner.writes.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }

    fn pty_resize(&self, rows: u16, cols: u16) -> Result<(), TerminalError> {
        self.inner.resizes.lock().unwrap().push((rows, cols));
        Ok(())
    }

    fn pty_close(&self) -> Result<(), TerminalError> {
        self.inner.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}
