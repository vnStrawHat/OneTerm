//! `impl TerminalSession for LocalSession` — render, input, mouse/selection,
//! clipboard, scroll, IME, và lifecycle query methods.

use std::path::PathBuf;

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use async_channel::Receiver;

use myterm2_core::terminal::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use myterm2_core::terminal::{TerminalContent, TerminalInfo};
use myterm2_core::{CursorBounds, SessionEvent, TerminalSession};

use crate::session::{LocalSession, TermSize};

impl TerminalSession for LocalSession {
    // ── Render ──────────────────────────────────────────────────────
    fn snapshot(&self) -> TerminalContent {
        let mut term = self.term.lock();
        TerminalContent::from(&mut *term)
    }

    fn terminal_info(&self) -> TerminalInfo {
        let term = self.term.lock();
        TerminalInfo {
            total_lines: term.total_lines(),
            cursor_line: term.grid().cursor.point.line.0,
            num_lines: term.screen_lines(),
            display_offset: term.grid().display_offset(),
        }
    }

    fn is_alt_screen(&self) -> bool {
        self.mode().contains(TermMode::ALT_SCREEN)
    }

    // ── Input ───────────────────────────────────────────────────────
    fn write(&self, bytes: &[u8]) {
        self.listener.pty_write(bytes);
    }

    fn flush_pty(&self) {
        // Gửi DSR (Device Status Report) query → ConPTY xử lý escape sequence,
        // respond với cursor position → flush output buffer.
        // Windows ConPTY buffer output, chỉ flush khi có interaction.
        self.listener.pty_write(b"\x1b[6n");
    }

    /// Gửi Ctrl+C signal đến shell process.
    ///
    /// Gửi \x03 qua PTY - ConPTY (với OpenConsole.exe từ Windows Terminal)
    /// xử lý signal routing đúng cách: CTRL_C_EVENT chỉ đến child process,
    /// không exit shell, không exit myTerm2.
    ///
    /// Yêu cầu: conpty.dll + OpenConsole.exe phải nằm cùng thư mục với exe.
    /// Xem crates/app/build.rs - tự copy từ assets/ ra target directory.
    #[cfg(windows)]
    fn send_ctrl_c(&self) {
        self.listener.pty_write(b"\x03");
    }

    #[cfg(not(windows))]
    fn send_ctrl_c(&self) {
        self.listener.pty_write(b"\x03");
    }

    fn resize(&self, rows: u16, cols: u16) {
        // Skip nếu size không đổi - tránh gửi pty_resize mỗi render (TerminalElement
        // được tạo lại mỗi frame, last_size luôn None). pty_resize không cần thiết
        // khi size giữ nguyên, và shell có thể redraw → clear selection.
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
            // Scroll đến top: delta = total_lines (scroll hết scrollback lên).
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
        if mode.contains(TermMode::MOUSE_MOTION) {
            let s = encode_mouse_move(
                row as usize,
                col as usize,
                None,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            // Button held không track ở trait signature - report hover (None).
            let s = encode_mouse_move(
                row as usize,
                col as usize,
                None,
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        }
        // Non-mouse mode: KHÔNG cập nhật selection - chỉ `mouse_drag` mới cập nhật.
    }

    fn mouse_drag(&self, row: f32, col: f32) {
        let mode = self.mode();
        if mode.intersects(TermMode::MOUSE_MODE) {
            // Mouse mode: encode drag với button Left.
            let s = encode_mouse_move(
                row as usize,
                col as usize,
                Some(TerminalMouseButton::Left),
                mode,
                MouseModifiers::default(),
            );
            self.write(s.as_bytes());
        } else {
            // Non-mouse mode: cập nhật selection end point.
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
        // Selection giữ nguyên để copy (clear khi click elsewhere - UI lo).
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
            // Alt-screen: wheel → arrow keys (app scroll, vd less/man).
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
        // Escape: clear visible screen + scrollback + home cursor.
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
        // Single-consumer: trả receiver nếu còn, không thì channel đóng.
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
        self.listener.pty_shutdown();
        self.state.lock().unwrap().alive = false;
    }

    fn is_local(&self) -> bool {
        true
    }

    fn title(&self) -> Option<String> {
        self.state.lock().unwrap().title.clone()
    }

    fn cwd(&self) -> Option<PathBuf> {
        self.state.lock().unwrap().cwd.clone()
    }

    // ── Shell Integration (OSC 133) ────────────────────────────
    fn prompt_count(&self) -> usize {
        self.state.lock().unwrap().prompt_count
    }
    fn scroll_to_prompt(&self, n: usize) {
        // TODO: implement scroll-to-prompt using prompt marker line positions.
        // For now, this is a placeholder - markers need grid line tracking.
        let _ = n;
    }

    // ── Foreground Process ─────────────────────────────────────
    fn foreground_process(&self) -> Option<String> {
        self.state.lock().unwrap().foreground_process.clone()
    }
}
