//! `LocalSession` - spawn shell cục bộ qua `alacritty_terminal::tty` +
//! `EventLoop` (ConPTY trên Windows).
//!
//! #11: spawn + struct + inherent methods. #12: `impl TerminalSession`
//! (mouse/selection/wheel + IME + cursor_bounds). Tham chiếu
//! `docs/terminal-backend.md` §6.2 + freya `handle.rs`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty::{self, Options, Shell};
use async_channel::Receiver;

use myterm2_core::config::resolve_shell;
use myterm2_core::terminal::TerminalContent;
use myterm2_core::terminal::mouse_encode::{
    MouseModifiers, TerminalMouseButton, encode_mouse_move, encode_mouse_press,
    encode_mouse_release, encode_wheel_event,
};
use myterm2_core::{AppError, CursorBounds, LocalShellConfig, SessionEvent, TerminalSession};

use crate::event_loop::ShellEventLoop;
use crate::listener::LocalListener;
use crate::state::{SharedState, new_shared};

/// Kích thước PTY ban đầu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

/// Dimensions cho `Term::new` / `Term::resize`.
struct TermSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for TermSize {
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

/// Một session shell cục bộ.
pub struct LocalSession {
    term: Arc<FairMutex<Term<LocalListener>>>,
    listener: LocalListener,
    event_rx: Mutex<Option<Receiver<SessionEvent>>>,
    state: SharedState,
    config: LocalShellConfig,
    /// Pixel cell metrics (UI set qua `set_cell_size`) - cho `cursor_bounds`.
    cell_width: Mutex<f32>,
    line_height: Mutex<f32>,
    /// IME marked text (compose buffer).
    marked_text: Mutex<Option<String>>,
}

impl LocalSession {
    /// Spawn shell theo `cfg` với kích thước ban đầu `initial`.
    pub fn spawn(cfg: LocalShellConfig, initial: PtySize) -> Result<Self, AppError> {
        let resolved = resolve_shell(&cfg)?;
        let opts = Options {
            shell: Some(Shell::new(
                resolved.program.to_string_lossy().into_owned(),
                resolved.args,
            )),
            working_directory: cfg.cwd.clone(),
            drain_on_exit: false,
            env: resolved.env,
            ..Default::default()
        };
        let winsize = WindowSize {
            num_lines: initial.rows,
            num_cols: initial.cols,
            cell_width: 0,
            cell_height: 0,
        };
        let pty = tty::new(&opts, winsize, 0).map_err(|e| AppError::msg(e.to_string()))?;

        let state = new_shared();
        state.lock().unwrap().alive = true;

        let (event_tx, event_rx) = async_channel::bounded::<SessionEvent>(4096);
        let listener = LocalListener::new(event_tx, state.clone());

        let size = TermSize {
            cols: initial.cols as usize,
            lines: initial.rows as usize,
        };
        let term_config = Config {
            scrolling_history: 10_000,
            ..Default::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            term_config,
            &size,
            listener.clone(),
        )));

        let (event_loop, notifier) =
            ShellEventLoop::new(pty, term.clone(), listener.clone(), state.clone())
                .map_err(|e| AppError::msg(e.to_string()))?;
        listener.set_notifier(notifier);
        let _join = event_loop.spawn();

        // Shell integration được inject qua env vars trong resolve_shell()
        // - hoàn toàn silent, không temp file, không viết script ra PTY.
        // Xem crates/core/src/config/shell.rs::resolve_shell().

        Ok(Self {
            term,
            listener,
            event_rx: Mutex::new(Some(event_rx)),
            state,
            config: cfg,
            cell_width: Mutex::new(0.0),
            line_height: Mutex::new(0.0),
            marked_text: Mutex::new(None),
        })
    }

    /// UI set pixel cell metrics (sau khi measure font) cho `cursor_bounds`.
    pub fn set_cell_size(&self, cell_width: f32, line_height: f32) {
        *self.cell_width.lock().unwrap() = cell_width;
        *self.line_height.lock().unwrap() = line_height;
    }

    /// Config đã spawn.
    pub fn config(&self) -> &LocalShellConfig {
        &self.config
    }

    // ── Helpers ──────────────────────────────────────────────────────
    /// Chuyển (row, col) pixel-cell → (Point, Side) để thao tác selection.
    fn point_and_side(term: &Term<LocalListener>, row: f32, col: f32) -> (Point, Side) {
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

    fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Bắt đầu selection (khi không ở mouse mode).
    fn start_selection(&self, row: f32, col: f32, sel: SelectionType) {
        let mut term = self.term.lock();
        let (point, side) = Self::point_and_side(&term, row, col);
        term.selection = Some(Selection::new(sel, point, side));
    }

    /// Cập nhật selection đang có (khi kéo).
    fn update_selection(&self, row: f32, col: f32) {
        let mut term = self.term.lock();
        // Compute point/side (immutable borrow) trước, rồi mới mutate selection.
        let (point, side) = Self::point_and_side(&term, row, col);
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
    }
}

impl TerminalSession for LocalSession {
    // ── Render ──────────────────────────────────────────────────────
    fn snapshot(&self) -> TerminalContent {
        let term = self.term.lock();
        TerminalContent::from(&*term)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_default() -> LocalSession {
        let cfg = LocalShellConfig::default();
        LocalSession::spawn(cfg, PtySize { rows: 24, cols: 80 }).expect("spawn")
    }

    #[test]
    fn trait_snapshot_bounds() {
        let s = spawn_default();
        let snap = s.snapshot();
        assert_eq!(snap.terminal_bounds.num_cols, 80);
        assert_eq!(snap.terminal_bounds.num_lines, 24);
        s.close();
    }

    #[test]
    fn trait_alive_is_local_close() {
        let s = spawn_default();
        assert!(s.alive());
        assert!(s.is_local());
        s.close();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!s.alive());
    }

    #[test]
    fn trait_subscribe_returns_receiver() {
        let s = spawn_default();
        let _rx = s.subscribe();
        // subscribe lần 2 → channel đóng (recv →Err Closed) nhưng không panic.
        let rx2 = s.subscribe();
        assert!(rx2.recv_blocking().is_err());
        s.close();
    }

    #[test]
    fn trait_write_resize_no_panic() {
        let s = spawn_default();
        s.write(b"echo hi\r");
        s.resize(30, 100);
        assert_eq!(s.snapshot().terminal_bounds.num_cols, 100);
        s.close();
    }

    #[test]
    fn trait_ime_commit_writes_and_clears_marked() {
        let s = spawn_default();
        s.set_marked_text("x".into());
        assert_eq!(s.marked_text().as_deref(), Some("x"));
        s.commit_text("hello");
        assert_eq!(s.marked_text(), None);
        s.close();
    }

    #[test]
    fn trait_cursor_bounds_needs_cell_size() {
        let s = spawn_default();
        // Chưa set_cell_size → None.
        assert_eq!(s.cursor_bounds(), None);
        s.set_cell_size(8.0, 16.0);
        let b = s.cursor_bounds();
        // Cursor visible default (mock có show cursor). Có thể None nếu Hidden.
        // Ít nhất không panic và trả dạng đúng khi có.
        if let Some(cb) = b {
            assert_eq!(cb.width, 8.0);
            assert_eq!(cb.height, 16.0);
        }
        s.close();
    }

    #[test]
    fn trait_mouse_in_normal_mode_starts_selection() {
        let s = spawn_default();
        // Cmd không bật mouse mode → selection (không panic, không encode).
        s.mouse_down(0.0, 0.0, TerminalMouseButton::Left, SelectionType::Simple);
        s.mouse_drag(0.0, 5.0);
        s.mouse_up(0.0, 5.0, TerminalMouseButton::Left);
        s.close();
    }

    #[test]
    fn selection_text_and_clear() {
        let s = spawn_default();
        // Bàn trống → chưa có selection.
        assert!(s.selection_text().is_none() || s.selection_text().as_deref() == Some(""));
        // Viết vài ký tự rồi select.
        s.write(b"hello");
        std::thread::sleep(std::time::Duration::from_millis(50));
        s.mouse_down(0.0, 0.0, TerminalMouseButton::Left, SelectionType::Simple);
        s.mouse_drag(0.0, 4.0);
        // selection_to_string có thể trả Some/None tùy trạng thái grid - chỉ kiểm không panic.
        let _ = s.selection_text();
        s.clear_selection();
        s.close();
    }

    #[test]
    fn mouse_drag_updates_selection_not_mouse_move() {
        let s = spawn_default();
        s.write(b"hello_world");
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Start selection at col 0
        s.mouse_down(0.0, 0.0, TerminalMouseButton::Left, SelectionType::Simple);
        // mouse_move (hover, no button) should NOT update selection
        s.mouse_move(0.0, 10.0);
        let snap = s.snapshot();
        // Selection should still be empty (start == end at col 0)
        // to_range returns None for empty simple selection
        assert!(
            snap.selection.is_none(),
            "mouse_move should not update selection"
        );
        // mouse_drag should update selection
        s.mouse_drag(0.0, 5.0);
        let snap2 = s.snapshot();
        assert!(
            snap2.selection.is_some(),
            "mouse_drag should update selection"
        );
        if let Some(sel) = &snap2.selection {
            assert_eq!(sel.start.column.0, 0);
            assert!(
                sel.end.column.0 >= 4,
                "end col should be >= 4 after drag to col 5"
            );
        }
        s.mouse_up(0.0, 5.0, TerminalMouseButton::Left);
        s.close();
    }

    #[test]
    fn trait_wheel_scroll_does_not_panic() {
        let s = spawn_default();
        s.wheel(3.0, 0.0, 0.0);
        s.wheel(-3.0, 0.0, 0.0);
        s.close();
    }

    #[test]
    fn set_cell_size_stores() {
        let s = spawn_default();
        s.set_cell_size(7.5, 15.0);
        assert_eq!(*s.cell_width.lock().unwrap(), 7.5);
        s.close();
    }

    #[test]
    fn spawn_cmd_exit_detected() {
        // Round-trip Windows: spawn cmd → write `exit\r` → ChildExit → alive false.
        use std::time::{Duration, Instant};
        let s = spawn_default();
        s.write(b"exit\r");
        let start = Instant::now();
        while s.alive() && start.elapsed() < Duration::from_secs(4) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!s.alive(), "cmd exit không được phát hiện sau 4s");
    }

    /// End-to-end (Windows): spawn cmd → write `echo myterm2_e2e` → poll
    /// snapshot → assert chuỗi xuất hiện trong grid cells (chứng minh toàn
    /// pipeline PTY→EventLoop→Term→snapshot hoạt động, không cần GUI).
    #[test]
    fn e2e_echo_output_rendered_in_snapshot() {
        use std::time::{Duration, Instant};
        let s = spawn_default();
        // Chờ prompt hiện ra một chút rồi gõ.
        std::thread::sleep(Duration::from_millis(200));
        s.write(b"echo myterm2_e2e\r");
        let needle = "myterm2_e2e";
        let start = Instant::now();
        let mut found = false;
        while start.elapsed() < Duration::from_secs(6) && !found {
            std::thread::sleep(Duration::from_millis(40));
            let snap = s.snapshot();
            // Gom ký tự từ tất cả cell (bỏ qua cell ' ' liên tiếp không cần).
            let text: String = snap.cells.iter().map(|ic| ic.cell.c).collect();
            found = text.contains(needle);
        }
        s.close();
        assert!(
            found,
            "`echo myterm2_e2e` không xuất hiện trong snapshot sau 6s"
        );
    }
}
