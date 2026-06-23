//! Tests for `LocalSession`.

use std::time::{Duration, Instant};

use alacritty_terminal::selection::SelectionType;
use myterm2_core::TerminalSession;
use myterm2_core::terminal::mouse_encode::TerminalMouseButton;

use crate::session::{LocalSession, PtySize};

fn spawn_default() -> LocalSession {
    let cfg = myterm2_core::LocalShellConfig::default();
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
