//! Deterministic terminal fakes for cross-crate tests and diagnostic harnesses.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use async_channel::{Receiver, Sender, TrySendError};

use crate::content::{IndexedCell, TermDamageInfo, TerminalBounds, TerminalContent};
use crate::mouse_encode::MouseModifiers;
use crate::mouse_encode::TerminalMouseButton;
use crate::osc_color::DynamicColors;
use crate::search::{SearchMatch, SearchOptions};
use crate::session::{
    LineRangeCells, SessionEvent, SessionKind, TerminalError, TerminalIme, TerminalInfo,
    TerminalInput, TerminalLifecycle, TerminalQueryState, TerminalRender, TerminalSession,
};

/// Whether a fake snapshot consumes the pending damage (render path) or leaves
/// it intact (auxiliary query path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DamageMode {
    /// Render snapshot — consume and reset the accumulated damage.
    Consume,
    /// Auxiliary query — do not touch the damage state.
    Preserve,
}

/// Shared observation and control handle for a [`FakeTerminalSession`].
#[derive(Clone)]
pub struct FakeSessionProbe {
    state: Arc<FakeSessionState>,
}

impl FakeSessionProbe {
    /// Replace the text returned by future snapshots.
    pub fn set_text(&self, text: impl Into<String>) {
        *self.state.text.lock().unwrap() = text.into();
        self.state.full_damage.store(true, Ordering::SeqCst);
    }

    /// Replace the terminal mode returned by future snapshots.
    pub fn set_mode(&self, mode: TermMode) {
        *self.state.mode.lock().unwrap() = mode;
        self.state.full_damage.store(true, Ordering::SeqCst);
    }

    /// Set the cursor position (display line, column) for future snapshots.
    pub fn set_cursor(&self, line: i32, col: usize) {
        *self.state.cursor.lock().unwrap() = (line, col);
        self.state.full_damage.store(true, Ordering::SeqCst);
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

    /// Return whether the fake session is alive.
    pub fn alive(&self) -> bool {
        self.state.alive.load(Ordering::SeqCst)
    }
}

/// A deterministic in-memory implementation of [`TerminalSession`].
pub struct FakeTerminalSession {
    state: Arc<FakeSessionState>,
    event_rx: Mutex<Option<Receiver<SessionEvent>>>,
}

struct FakeSessionState {
    rows_cols: Mutex<(usize, usize)>,
    text: Mutex<String>,
    mode: Mutex<TermMode>,
    cursor: Mutex<(i32, usize)>,
    writes: Mutex<Vec<Vec<u8>>>,
    event_tx: Sender<SessionEvent>,
    full_damage: AtomicBool,
    fail_writes: AtomicBool,
    alive: AtomicBool,
    snapshot_calls: AtomicUsize,
    close_calls: AtomicUsize,
}

impl FakeTerminalSession {
    /// Create a fake session and its observation probe.
    pub fn new(rows: usize, cols: usize, text: impl Into<String>) -> (Self, FakeSessionProbe) {
        let (event_tx, event_rx) = async_channel::bounded(64);
        let state = Arc::new(FakeSessionState {
            rows_cols: Mutex::new((rows.max(1), cols.max(1))),
            text: Mutex::new(text.into()),
            mode: Mutex::new(TermMode::SHOW_CURSOR),
            cursor: Mutex::new((0, 0)),
            writes: Mutex::new(Vec::new()),
            event_tx,
            full_damage: AtomicBool::new(true),
            fail_writes: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            snapshot_calls: AtomicUsize::new(0),
            close_calls: AtomicUsize::new(0),
        });
        let probe = FakeSessionProbe {
            state: state.clone(),
        };
        (
            Self {
                state,
                event_rx: Mutex::new(Some(event_rx)),
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
        let (session, probe) = Self::new(rows, cols, text);
        (Box::new(session), probe)
    }

    fn content(&self, damage: DamageMode) -> TerminalContent {
        let (rows, cols) = *self.state.rows_cols.lock().unwrap();
        let text = self.state.text.lock().unwrap().clone();
        let mode = *self.state.mode.lock().unwrap();
        let mut characters = vec![' '; rows * cols];
        for (row, line) in text.lines().take(rows).enumerate() {
            for (col, character) in line.chars().take(cols).enumerate() {
                characters[row * cols + col] = character;
            }
        }

        let cells = characters
            .into_iter()
            .enumerate()
            .map(|(index, character)| {
                let row = index / cols;
                let col = index % cols;
                let mut cell = Cell::default();
                cell.c = character;
                IndexedCell {
                    point: Point::new(Line(row as i32), Column(col)),
                    cell,
                }
            })
            .collect();

        let damage = if damage == DamageMode::Consume
            && self.state.full_damage.swap(false, Ordering::SeqCst)
        {
            TermDamageInfo::Full
        } else {
            TermDamageInfo::Partial(Vec::new())
        };

        let (cursor_line, cursor_col) = *self.state.cursor.lock().unwrap();
        TerminalContent {
            cells,
            cursor: RenderableCursor {
                shape: CursorShape::Block,
                point: Point::new(Line(cursor_line), Column(cursor_col)),
            },
            mode,
            display_offset: 0,
            total_lines: rows,
            selection: None,
            terminal_bounds: TerminalBounds {
                num_lines: rows,
                num_cols: cols,
            },
            damage,
        }
    }
}

impl TerminalSession for FakeTerminalSession {}

impl TerminalRender for FakeTerminalSession {
    fn snapshot(&self) -> TerminalContent {
        self.state.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        self.content(DamageMode::Consume)
    }

    fn snapshot_query(&self) -> TerminalContent {
        self.content(DamageMode::Preserve)
    }

    fn query_state(&self) -> TerminalQueryState {
        let mode = *self.state.mode.lock().unwrap();
        let snap = self.content(DamageMode::Preserve);
        TerminalQueryState {
            mode,
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

    fn query_line_range_cells(&self, start_line: usize, count: usize) -> LineRangeCells {
        let snap = self.content(DamageMode::Preserve);
        let num_cols = snap.terminal_bounds.num_cols;
        let start = start_line * num_cols;
        let end = (start + count * num_cols).min(snap.cells.len());
        let cells = if start <= snap.cells.len() {
            snap.cells[start..end].to_vec()
        } else {
            Vec::new()
        };
        LineRangeCells { cells, num_cols }
    }

    fn terminal_info(&self) -> TerminalInfo {
        let (rows, cols) = *self.state.rows_cols.lock().unwrap();
        TerminalInfo {
            total_lines: rows,
            absolute_line_count: rows,
            cursor_line: 0,
            last_content_line: self
                .state
                .text
                .lock()
                .unwrap()
                .lines()
                .count()
                .saturating_sub(1) as i32,
            num_lines: rows,
            num_cols: cols,
            display_offset: 0,
            clear_epoch: 0,
        }
    }

    fn is_alt_screen(&self) -> bool {
        self.state
            .mode
            .lock()
            .unwrap()
            .contains(TermMode::ALT_SCREEN)
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
        None
    }

    fn has_selection(&self) -> bool {
        false
    }
}

impl TerminalInput for FakeTerminalSession {
    fn write(&self, bytes: &[u8]) -> Result<(), TerminalError> {
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
        *self.state.rows_cols.lock().unwrap() = (rows.max(1) as usize, cols.max(1) as usize);
        self.state.full_damage.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn scroll(&self, _delta: i32) {}

    fn scroll_to_bottom(&self) {}

    fn scroll_to_top(&self) {}

    fn mouse_down(
        &self,
        _row: f32,
        _col: f32,
        _button: TerminalMouseButton,
        _selection: SelectionType,
        _mods: MouseModifiers,
    ) {
    }

    fn mouse_move(&self, _row: f32, _col: f32, _mods: MouseModifiers) {}

    fn mouse_drag(&self, _row: f32, _col: f32, _mods: MouseModifiers) {}

    fn mouse_up(&self, _row: f32, _col: f32, _button: TerminalMouseButton, _mods: MouseModifiers) {}

    fn wheel(&self, _delta_y: f64, _row: f32, _col: f32, _mods: MouseModifiers) {}

    fn clear_selection(&self) {}

    fn select_all(&self) {}

    fn clear(&self) {}
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
        SessionKind::Local
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
