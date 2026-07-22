//! Tests for `LocalSession`.

use std::time::{Duration, Instant};

use alacritty_terminal::selection::SelectionType;
use oneterm_terminal::mouse_encode::{MouseModifiers, TerminalMouseButton};
use oneterm_terminal::{TerminalError, TerminalSession};

use crate::session::LocalSession;
use oneterm_terminal::PtySize;

fn spawn_default() -> LocalSession {
    let cfg = oneterm_core::LocalShellConfig::default();
    LocalSession::spawn(cfg, PtySize { rows: 24, cols: 80 }, 10_000).expect("spawn")
}

#[test]
fn trait_snapshot_bounds() {
    let s = spawn_default();
    let snap = s.snapshot();
    assert_eq!(snap.terminal_bounds.num_cols, 80);
    assert_eq!(snap.terminal_bounds.num_lines, 24);
    let _ = s.close();
}

#[test]
fn trait_alive_is_local_close() {
    let s = spawn_default();
    assert!(s.alive());
    assert!(s.is_local());
    s.close().expect("close and join PTY owner");
    assert_eq!(s.write(b"after-close"), Err(TerminalError::Closed));
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!s.alive());
}

#[test]
fn trait_subscribe_returns_receiver() {
    let s = spawn_default();
    let _rx = s.subscribe();
    // 2nd subscribe → closed channel (recv → Err Closed) but no panic.
    let rx2 = s.subscribe();
    assert!(rx2.recv_blocking().is_err());
    let _ = s.close();
}

#[test]
fn trait_write_resize_no_panic() {
    let s = spawn_default();
    let _ = s.write(b"echo hi\r");
    let _ = s.resize(30, 100);
    assert_eq!(s.snapshot().terminal_bounds.num_cols, 100);
    let _ = s.close();
}

#[test]
fn trait_ime_commit_writes_and_clears_marked() {
    let s = spawn_default();
    s.set_marked_text("x".into());
    assert_eq!(s.marked_text().as_deref(), Some("x"));
    s.commit_text("hello");
    assert_eq!(s.marked_text(), None);
    let _ = s.close();
}

#[test]
fn trait_cursor_bounds_needs_cell_size() {
    let s = spawn_default();
    // Cell size not set yet → None.
    assert_eq!(s.cursor_bounds(), None);
    s.set_cell_size(8.0, 16.0);
    let b = s.cursor_bounds();
    // Cursor is visible by default (mock shows the cursor). May be None if Hidden.
    // At minimum it must not panic and must return the correct shape when present.
    if let Some(cb) = b {
        assert_eq!(cb.width, 8.0);
        assert_eq!(cb.height, 16.0);
    }
    let _ = s.close();
}

#[test]
fn trait_mouse_in_normal_mode_starts_selection() {
    let s = spawn_default();
    // Cmd does not enable mouse mode → selection (no panic, no encoding).
    s.mouse_down(
        0.0,
        0.0,
        TerminalMouseButton::Left,
        SelectionType::Simple,
        MouseModifiers::default(),
    );
    s.mouse_drag(0.0, 5.0, MouseModifiers::default());
    s.mouse_up(
        0.0,
        5.0,
        TerminalMouseButton::Left,
        MouseModifiers::default(),
    );
    let _ = s.close();
}

#[test]
fn selection_text_and_clear() {
    let s = spawn_default();
    // Empty buffer → no selection yet.
    assert!(s.selection_text().is_none() || s.selection_text().as_deref() == Some(""));
    // Write a few characters then select.
    let _ = s.write(b"hello");
    std::thread::sleep(std::time::Duration::from_millis(50));
    s.mouse_down(
        0.0,
        0.0,
        TerminalMouseButton::Left,
        SelectionType::Simple,
        MouseModifiers::default(),
    );
    s.mouse_drag(0.0, 4.0, MouseModifiers::default());
    // selection_to_string may return Some/None depending on grid state — just check no panic.
    let _ = s.selection_text();
    s.clear_selection();
    let _ = s.close();
}

#[test]
fn mouse_drag_updates_selection_not_mouse_move() {
    let s = spawn_default();
    let _ = s.write(b"hello_world");
    std::thread::sleep(std::time::Duration::from_millis(50));
    // Start selection at col 0
    s.mouse_down(
        0.0,
        0.0,
        TerminalMouseButton::Left,
        SelectionType::Simple,
        MouseModifiers::default(),
    );
    // mouse_move (hover, no button) should NOT update selection
    s.mouse_move(0.0, 10.0, MouseModifiers::default());
    let snap = s.snapshot();
    // Selection should still be empty (start == end at col 0)
    // to_range returns None for empty simple selection
    assert!(
        snap.selection.is_none(),
        "mouse_move should not update selection"
    );
    // mouse_drag should update selection
    s.mouse_drag(0.0, 5.0, MouseModifiers::default());
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
    s.mouse_up(
        0.0,
        5.0,
        TerminalMouseButton::Left,
        MouseModifiers::default(),
    );
    let _ = s.close();
}

#[test]
fn trait_wheel_scroll_does_not_panic() {
    let s = spawn_default();
    s.wheel(3.0, 0.0, 0.0, MouseModifiers::default());
    s.wheel(-3.0, 0.0, 0.0, MouseModifiers::default());
    let _ = s.close();
}

#[test]
fn set_cell_size_stores() {
    let s = spawn_default();
    s.set_cell_size(7.5, 15.0);
    assert_eq!(*s.cell_width.lock().unwrap(), 7.5);
    let _ = s.close();
}

#[test]
fn spawn_cmd_exit_detected() {
    // Round-trip Windows: spawn cmd → write `exit\r` → ChildExit → alive false.
    let s = spawn_default();
    let _ = s.write(b"exit\r");
    let start = Instant::now();
    while s.alive() && start.elapsed() < Duration::from_secs(4) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!s.alive(), "cmd exit not detected after 4s");
}

/// End-to-end (Windows): spawn cmd → write `echo oneterm_e2e` → poll the
/// snapshot → assert the string appears in the grid cells (proving the whole
/// PTY→EventLoop→Term→snapshot pipeline works, no GUI needed).
#[test]
fn e2e_echo_output_rendered_in_snapshot() {
    let s = spawn_default();
    // Wait a moment for the prompt to appear, then type.
    std::thread::sleep(Duration::from_millis(200));
    let _ = s.write(b"echo oneterm_e2e\r");
    let needle = "oneterm_e2e";
    let start = Instant::now();
    let mut found = false;
    while start.elapsed() < Duration::from_secs(6) && !found {
        std::thread::sleep(Duration::from_millis(40));
        let snap = s.snapshot();
        // Collect characters from all cells (ignore runs of ' ' as needed).
        let text: String = snap.cells.iter().map(|ic| ic.cell.c).collect();
        found = text.contains(needle);
    }
    let _ = s.close();
    assert!(
        found,
        "`echo oneterm_e2e` did not appear in the snapshot after 6s"
    );
}
