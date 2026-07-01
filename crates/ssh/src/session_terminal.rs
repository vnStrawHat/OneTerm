//! `impl TerminalSession for SshSession` — render, input, mouse/selection,
//! clipboard, scroll, IME, and lifecycle query methods.
//!
//! Similar to `local/src/session_terminal.rs` but for the SSH session.

use std::path::PathBuf;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use async_channel::Receiver;

use oneterm_core::terminal::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use oneterm_core::terminal::{TerminalContent, TerminalInfo};
use oneterm_core::{CursorBounds, SessionEvent, SftpBackend, TerminalSession};

use crate::session::{SshSession, TermSize};

impl SshSession {
    /// UI sets pixel cell metrics (after measuring the font) for `cursor_bounds`.
    pub fn set_cell_size(&self, cell_width: f32, line_height: f32) {
        *self.cell_width.lock().unwrap() = cell_width;
        *self.line_height.lock().unwrap() = line_height;
    }

    // ── Helpers ──────────────────────────────────────────────────────

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    fn start_selection(&self, row: f32, col: f32, sel: SelectionType) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset() as i32;
        let row_idx = (row.max(0.0) as usize).min(term.screen_lines().saturating_sub(1));
        let column = (col.max(0.0) as usize).min(term.columns().saturating_sub(1));
        let line = row_idx as i32 - display_offset;
        let side = if col.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        let point = Point::new(Line(line), Column(column));
        term.selection = Some(Selection::new(sel, point, side));
    }

    fn update_selection(&self, row: f32, col: f32) {
        let mut term = self.term.lock();
        let display_offset = term.grid().display_offset() as i32;
        let row_idx = (row.max(0.0) as usize).min(term.screen_lines().saturating_sub(1));
        let column = (col.max(0.0) as usize).min(term.columns().saturating_sub(1));
        let line = row_idx as i32 - display_offset;
        let side = if col.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        let point = Point::new(Line(line), Column(column));
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }
}

impl TerminalSession for SshSession {
    // ── Render ──────────────────────────────────────────────────────
    fn snapshot(&self) -> TerminalContent {
        let mut term = self.term.lock();
        TerminalContent::from(&mut *term)
    }

    fn terminal_info(&self) -> TerminalInfo {
        let term = self.term.lock();
        let total_lines = term.total_lines();
        let st = self.state.lock().unwrap();
        TerminalInfo {
            total_lines,
            absolute_line_count: st.absolute_line_count.max(total_lines),
            cursor_line: term.grid().cursor.point.line.0,
            last_content_line: oneterm_core::terminal::last_content_line(&term),
            num_lines: term.screen_lines(),
            display_offset: term.grid().display_offset(),
            clear_epoch: st.clear_epoch,
        }
    }

    fn is_alt_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    // ── Input ───────────────────────────────────────────────────────
    fn write(&self, bytes: &[u8]) {
        log::trace!(
            "SshSession::write: {} bytes: {:?}",
            bytes.len(),
            String::from_utf8_lossy(bytes)
        );
        self.listener.pty_write(bytes);
    }

    fn flush_pty(&self) {
        // SSH needs no ConPTY workaround — send a DSR query.
        self.listener.pty_write(b"\x1b[6n");
    }

    fn send_ctrl_c(&self) {
        self.listener.pty_write(b"\x03");
    }

    fn resize(&self, rows: u16, cols: u16) {
        let needs_resize = {
            let term = self.term.lock();
            term.columns() != cols as usize || term.screen_lines() != rows as usize
        };
        if !needs_resize {
            return;
        }
        self.listener.pty_resize(rows, cols);
        self.term.lock().resize(TermSize {
            cols: cols as usize,
            lines: rows as usize,
        });
    }

    fn scroll(&self, delta: i32) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            term.scroll_display(Scroll::Delta(delta));
        }
    }

    fn scroll_to_bottom(&self) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            term.scroll_display(Scroll::Bottom);
        }
    }

    fn scroll_to_top(&self) {
        let mut term = self.term.lock();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            let total = term.total_lines() as i32;
            term.scroll_display(Scroll::Delta(total));
        }
    }

    // ── Mouse ────────────────────────────────────────────────────────
    fn mouse_down(&self, row: f32, col: f32, button: TerminalMouseButton, sel: SelectionType) {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let s = encode_mouse_press(
                row as usize,
                col as usize,
                button,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        } else {
            self.start_selection(row, col, sel);
        }
    }

    fn mouse_move(&self, row: f32, col: f32) {
        let mode = self.mode();
        if mode.contains(TermMode::MOUSE_MOTION) || mode.contains(TermMode::MOUSE_DRAG) {
            let s = encode_mouse_move(
                row as usize,
                col as usize,
                None,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        }
    }

    fn mouse_drag(&self, row: f32, col: f32) {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let s = encode_mouse_move(
                row as usize,
                col as usize,
                Some(TerminalMouseButton::Left),
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        } else {
            self.update_selection(row, col);
        }
    }

    fn mouse_up(&self, row: f32, col: f32, button: TerminalMouseButton) {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            let s = encode_mouse_release(
                row as usize,
                col as usize,
                button,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        }
    }

    fn wheel(&self, delta_y: f64, row: f32, col: f32) {
        let lines = (delta_y.abs().ceil() as i32).clamp(1, 10);
        let scroll_delta = if delta_y > 0.0 { lines } else { -lines };
        let mode = self.mode();
        let display_offset = self.term.lock().grid().display_offset();

        if display_offset > 0 {
            self.scroll(scroll_delta);
        } else if mode.intersects(TermMode::MOUSE_MODE) {
            let s = encode_wheel_event(
                row as usize,
                col as usize,
                delta_y,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        } else if mode.contains(TermMode::ALT_SCREEN) {
            let app_cursor = mode.contains(TermMode::APP_CURSOR);
            let key = match (delta_y > 0.0, app_cursor) {
                (true, true) => "\x1bOA",
                (true, false) => "\x1b[A",
                (false, true) => "\x1bOB",
                (false, false) => "\x1b[B",
            };
            for _ in 0..lines {
                self.write(key.as_bytes());
            }
        } else {
            self.scroll(scroll_delta);
        }
    }

    // ── Selection / clipboard ───────────────────────────────────────
    fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    fn select_all(&self) {
        let mut term = self.term.lock();
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
        sel.update(end, Side::Right);
        term.selection = Some(sel);
    }

    fn clear(&self) {
        self.write(b"\x1b[3J\x1b[2J\x1b[H");
        self.clear_selection();
    }

    // ── IME ──────────────────────────────────────────────────────────
    fn set_marked_text(&self, text: String) {
        *self.marked_text.lock().unwrap() = Some(text);
    }

    fn clear_marked_text(&self) {
        *self.marked_text.lock().unwrap() = None;
    }

    fn commit_text(&self, text: &str) {
        self.clear_marked_text();
        self.write(text.as_bytes());
    }

    fn marked_text(&self) -> Option<String> {
        self.marked_text.lock().unwrap().clone()
    }

    fn cursor_bounds(&self) -> Option<CursorBounds> {
        let cw = *self.cell_width.lock().unwrap();
        let lh = *self.line_height.lock().unwrap();
        if cw <= 0.0 || lh <= 0.0 {
            return None;
        }
        let snap = self.snapshot();
        let cursor = snap.cursor;
        if matches!(
            cursor.shape,
            alacritty_terminal::vte::ansi::CursorShape::Hidden
        ) {
            return None;
        }
        let col = cursor.point.column.0 as f32;
        let line = (cursor.point.line.0 + snap.display_offset as i32) as f32;
        Some(CursorBounds {
            x: col * cw,
            y: line * lh,
            width: cw,
            height: lh,
        })
    }

    // ── Lifecycle ────────────────────────────────────────────────────
    fn subscribe(&self) -> Receiver<SessionEvent> {
        self.event_rx
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| async_channel::bounded(1).1)
    }

    fn alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }

    fn close(&self) {
        self.listener.pty_close();
        self.state.lock().unwrap().alive = false;
    }

    fn is_local(&self) -> bool {
        false
    }

    fn title(&self) -> Option<String> {
        self.state.lock().unwrap().title.clone()
    }

    fn cwd(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().cwd.clone()
    }

    // ── Shell Integration ───────────────────────────────────────────
    fn prompt_count(&self) -> usize {
        self.state.lock().unwrap().prompt_count
    }

    // ── Foreground Process ───────────────────────────────────────────
    fn foreground_process(&self) -> Option<String> {
        self.state.lock().unwrap().foreground_process.clone()
    }

    // ── Network Stats ───────────────────────────────────────────
    fn network_stats(&self) -> Option<oneterm_core::NetStats> {
        let st = self.state.lock().unwrap();
        Some(oneterm_core::NetStats {
            rx_bytes: st.rx_bytes,
            tx_bytes: st.tx_bytes,
        })
    }

    // ── SFTP ────────────────────────────────────────────────
    fn sftp(&self) -> Option<std::sync::Arc<dyn SftpBackend>> {
        self.sftp
            .lock()
            .unwrap()
            .clone()
            .map(|s| s as std::sync::Arc<dyn SftpBackend>)
    }
}
