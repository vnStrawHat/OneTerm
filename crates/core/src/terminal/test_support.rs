//! Deterministic terminal fakes for cross-crate tests and diagnostic harnesses.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::term::{RenderableCursor, TermMode};
use alacritty_terminal::vte::ansi::CursorShape;
use async_channel::{Receiver, Sender, TryRecvError, TrySendError};

use super::{
    CursorBounds, IndexedCell, SessionEvent, TermDamageInfo, TerminalBounds, TerminalContent,
    TerminalInfo, TerminalMouseButton, TerminalSession,
};
use crate::terminal::mouse_encode::MouseModifiers;

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

    /// Send an event to the session subscriber.
    pub fn emit(&self, event: SessionEvent) -> Result<(), TrySendError<SessionEvent>> {
        self.state.event_tx.try_send(event)
    }

    /// Return every write captured by the fake transport.
    pub fn writes(&self) -> Vec<Vec<u8>> {
        self.state.writes.lock().unwrap().clone()
    }

    /// Remove and return all captured writes.
    pub fn take_writes(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.state.writes.lock().unwrap())
    }

    /// Return the number of render snapshots requested.
    pub fn snapshot_calls(&self) -> usize {
        self.state.snapshot_calls.load(Ordering::SeqCst)
    }

    /// Return the number of damage-free query snapshots requested.
    pub fn query_snapshot_calls(&self) -> usize {
        self.state.query_snapshot_calls.load(Ordering::SeqCst)
    }

    /// Return the number of terminal-info reads requested.
    pub fn terminal_info_calls(&self) -> usize {
        self.state.terminal_info_calls.load(Ordering::SeqCst)
    }

    /// Return the number of close requests received by the fake.
    pub fn close_calls(&self) -> usize {
        self.state.close_calls.load(Ordering::SeqCst)
    }

    /// Return the number of times the fake session object was dropped.
    pub fn drop_calls(&self) -> usize {
        self.state.drop_calls.load(Ordering::SeqCst)
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
    writes: Mutex<Vec<Vec<u8>>>,
    event_tx: Sender<SessionEvent>,
    full_damage: AtomicBool,
    alive: AtomicBool,
    snapshot_calls: AtomicUsize,
    query_snapshot_calls: AtomicUsize,
    terminal_info_calls: AtomicUsize,
    close_calls: AtomicUsize,
    drop_calls: AtomicUsize,
}

impl FakeTerminalSession {
    /// Create a fake session and its observation probe.
    pub fn new(rows: usize, cols: usize, text: impl Into<String>) -> (Self, FakeSessionProbe) {
        let (event_tx, event_rx) = async_channel::bounded(64);
        let state = Arc::new(FakeSessionState {
            rows_cols: Mutex::new((rows.max(1), cols.max(1))),
            text: Mutex::new(text.into()),
            mode: Mutex::new(TermMode::SHOW_CURSOR),
            writes: Mutex::new(Vec::new()),
            event_tx,
            full_damage: AtomicBool::new(true),
            alive: AtomicBool::new(true),
            snapshot_calls: AtomicUsize::new(0),
            query_snapshot_calls: AtomicUsize::new(0),
            terminal_info_calls: AtomicUsize::new(0),
            close_calls: AtomicUsize::new(0),
            drop_calls: AtomicUsize::new(0),
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

    fn content(&self, consume_damage: bool) -> TerminalContent {
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

        let damage = if consume_damage && self.state.full_damage.swap(false, Ordering::SeqCst) {
            TermDamageInfo::Full
        } else {
            TermDamageInfo::Partial(Vec::new())
        };

        TerminalContent {
            cells,
            cursor: RenderableCursor {
                shape: CursorShape::Block,
                point: Point::new(Line(0), Column(0)),
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

impl Drop for FakeTerminalSession {
    fn drop(&mut self) {
        self.state.drop_calls.fetch_add(1, Ordering::SeqCst);
    }
}

impl TerminalSession for FakeTerminalSession {
    fn snapshot(&self) -> TerminalContent {
        self.state.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        self.content(true)
    }

    fn snapshot_query(&self) -> TerminalContent {
        self.state
            .query_snapshot_calls
            .fetch_add(1, Ordering::SeqCst);
        self.content(false)
    }

    fn terminal_info(&self) -> TerminalInfo {
        self.state
            .terminal_info_calls
            .fetch_add(1, Ordering::SeqCst);
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

    fn write(&self, bytes: &[u8]) {
        self.state.writes.lock().unwrap().push(bytes.to_vec());
    }

    fn flush_pty(&self) {}

    fn send_ctrl_c(&self) {
        self.write(b"\x03");
    }

    fn resize(&self, rows: u16, cols: u16) {
        *self.state.rows_cols.lock().unwrap() = (rows.max(1) as usize, cols.max(1) as usize);
        self.state.full_damage.store(true, Ordering::SeqCst);
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

    fn selection_text(&self) -> Option<String> {
        None
    }

    fn clear_selection(&self) {}

    fn select_all(&self) {}

    fn clear(&self) {}

    fn set_marked_text(&self, _text: String) {}

    fn clear_marked_text(&self) {}

    fn commit_text(&self, text: &str) {
        self.write(text.as_bytes());
    }

    fn marked_text(&self) -> Option<String> {
        None
    }

    fn cursor_bounds(&self) -> Option<CursorBounds> {
        None
    }

    fn subscribe(&self) -> Receiver<SessionEvent> {
        self.event_rx
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| async_channel::bounded(1).1)
    }

    fn alive(&self) -> bool {
        self.state.alive.load(Ordering::SeqCst)
    }

    fn close(&self) {
        self.state.close_calls.fetch_add(1, Ordering::SeqCst);
        self.state.alive.store(false, Ordering::SeqCst);
        let _ = self.state.event_tx.try_send(SessionEvent::Closed);
    }

    fn is_local(&self) -> bool {
        true
    }

    fn title(&self) -> Option<String> {
        None
    }

    fn cwd(&self) -> Option<PathBuf> {
        None
    }
}

/// A bounded in-memory transport used to drive saturation tests.
pub struct FakeTransport<T> {
    sender: Sender<T>,
    receiver: Receiver<T>,
}

impl<T> FakeTransport<T> {
    /// Create a bounded fake transport.
    pub fn bounded(capacity: usize) -> Self {
        let (sender, receiver) = async_channel::bounded(capacity);
        Self { sender, receiver }
    }

    /// Clone the transport sender for injection into a backend adapter.
    pub fn sender(&self) -> Sender<T> {
        self.sender.clone()
    }

    /// Try to enqueue one item.
    pub fn try_send(&self, item: T) -> Result<(), TrySendError<T>> {
        self.sender.try_send(item)
    }

    /// Try to receive one item.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.receiver.try_recv()
    }

    /// Close both ends of the fake transport.
    pub fn close(&self) {
        self.sender.close();
        self.receiver.close();
    }

    /// Return the current number of queued items.
    pub fn len(&self) -> usize {
        self.receiver.len()
    }

    /// Return whether the transport queue is empty.
    pub fn is_empty(&self) -> bool {
        self.receiver.is_empty()
    }
}
