//! Tests for `LocalSession`.

use std::time::{Duration, Instant};

use alacritty_terminal::selection::SelectionType;
use oneterm_terminal::mouse_encode::{MouseModifiers, TerminalMouseButton};
use oneterm_terminal::{TerminalError, TerminalSecurityPolicy, TerminalSession};

use crate::session::{LocalSession, quote_windows_argument};
use oneterm_terminal::PtySize;

#[test]
fn program_path_with_spaces_is_quoted_for_conpty() {
    assert_eq!(
        quote_windows_argument(r"C:\Program Files\PowerShell\7\pwsh.exe"),
        r#""C:\Program Files\PowerShell\7\pwsh.exe""#
    );
    // A trailing backslash must be doubled so it does not escape the closing quote.
    assert_eq!(
        quote_windows_argument(r"C:\Program Files\"),
        r#""C:\Program Files\\""#
    );
    assert_eq!(quote_windows_argument(""), r#""""#);
}

#[test]
fn cmd_utf8_command_line_stays_verbatim_under_escaping() {
    // `cmd /K chcp 65001 >nul`: neither the program nor the arguments contain
    // whitespace or quotes, so CRT escaping leaves cmd.exe's `/K` command line
    // untouched — this is why `escape_args` can be enabled unconditionally.
    assert_eq!(
        quote_windows_argument(r"C:\Windows\System32\cmd.exe"),
        r"C:\Windows\System32\cmd.exe"
    );
    for arg in ["/K", "chcp", "65001", ">nul"] {
        assert_eq!(quote_windows_argument(arg), arg);
    }
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
    predicate()
}

fn snapshot_contains(session: &LocalSession, needle: &str) -> bool {
    session
        .snapshot()
        .cells
        .iter()
        .map(|indexed| indexed.cell.c)
        .collect::<String>()
        .contains(needle)
}

fn spawn_default() -> LocalSession {
    let cfg = oneterm_core::LocalShellConfig::default();
    LocalSession::spawn(
        cfg,
        PtySize { rows: 24, cols: 80 },
        10_000,
        TerminalSecurityPolicy::default(),
    )
    .expect("spawn")
}

#[cfg(windows)]
fn assert_powershell_prompt_emits_cwd(kind: oneterm_core::ShellKind, label: &str) {
    let cfg = oneterm_core::LocalShellConfig {
        kind,
        ..Default::default()
    };
    let session = LocalSession::spawn(
        cfg,
        PtySize { rows: 24, cols: 80 },
        10_000,
        TerminalSecurityPolicy::default(),
    )
    .unwrap_or_else(|error| panic!("spawn {label}: {error}"));

    let emitted_cwd = wait_until(Duration::from_secs(15), || session.cwd().is_some());
    let snapshot = session
        .snapshot_query()
        .cells
        .iter()
        .map(|indexed| indexed.cell.c)
        .collect::<String>();
    assert!(
        emitted_cwd,
        "{label} prompt must emit OSC 7 through the PTY; terminal snapshot: {snapshot}"
    );
    assert!(!snapshot.contains("ParserError"), "{snapshot}");
    assert!(
        !snapshot.contains("Missing ')' in method call"),
        "{snapshot}"
    );
    session
        .close()
        .unwrap_or_else(|error| panic!("close {label}: {error}"));
}

#[cfg(windows)]
#[test]
fn windows_powershell_prompt_emits_cwd_without_parser_errors() {
    assert_powershell_prompt_emits_cwd(oneterm_core::ShellKind::PowerShell, "PowerShell");
}

#[cfg(windows)]
#[test]
fn pwsh_prompt_emits_cwd_without_parser_errors() {
    assert_powershell_prompt_emits_cwd(oneterm_core::ShellKind::Pwsh, "pwsh");
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
    assert!(wait_until(Duration::from_secs(2), || !s.alive()));
}

/// CORR-10: `close()` must not join the PTY owner thread on the caller's
/// thread. Hold the `Term` lock (which the owner needs to make progress) and
/// require `close()` to return promptly regardless; the reaper thread joins
/// the owner afterwards.
#[test]
fn close_returns_without_joining_the_owner_thread() {
    let s = spawn_default();
    let _ = s.write(b"echo hold\r");
    let guard = s.term.lock();
    let started = Instant::now();
    s.close().expect("close must succeed");
    let elapsed = started.elapsed();
    drop(guard);
    assert!(
        elapsed < Duration::from_millis(500),
        "close() blocked for {elapsed:?} — it must hand the owner thread to the reaper"
    );
    assert!(
        s.owner_join.lock().unwrap().is_none(),
        "the join handle must have been handed off"
    );
    assert!(wait_until(Duration::from_secs(2), || !s.alive()));
}

/// ARCH-06: a program that cannot be started reports a typed
/// `ShellResolution` error naming the program.
#[cfg(windows)]
#[test]
fn spawn_failure_is_a_typed_shell_resolution_error() {
    let cfg = oneterm_core::LocalShellConfig {
        kind: oneterm_core::ShellKind::Custom,
        program: Some(std::path::PathBuf::from(
            r"C:\oneterm-does-not-exist\no-such-shell.exe",
        )),
        ..Default::default()
    };
    let error = LocalSession::spawn(cfg, PtySize { rows: 24, cols: 80 }, 10_000)
        .err()
        .expect("spawning a missing program must fail");
    match error {
        oneterm_core::AppError::ShellResolution { shell, .. } => {
            assert!(shell.contains("no-such-shell.exe"), "{shell}");
        }
        other => panic!("expected ShellResolution, got {other:?}"),
    }
}

#[test]
fn trait_take_events_hands_out_the_receiver_once() {
    let s = spawn_default();
    let first = s.take_events();
    assert!(first.is_some());
    // The single receiver is gone: a second consumer gets nothing instead of
    // a dead channel that would silently miss every event.
    assert!(s.take_events().is_none());
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
    assert!(wait_until(Duration::from_secs(2), || snapshot_contains(
        &s, "hello"
    )));
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
    assert!(wait_until(Duration::from_secs(2), || {
        snapshot_contains(&s, "hello_world")
    }));
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
fn spawned_shell_exit_is_detected() {
    // Cross-platform round trip: request shell exit and observe lifecycle state.
    let s = spawn_default();
    let _ = s.write(b"exit\r");
    assert!(
        wait_until(Duration::from_secs(4), || !s.alive()),
        "shell exit not detected after 4s"
    );
}

/// End-to-end: spawn the platform shell, write `echo oneterm_e2e`, and assert
/// that the text reaches the terminal snapshot without requiring a GUI.
#[test]
fn e2e_echo_output_rendered_in_snapshot() {
    let s = spawn_default();
    let _ = s.write(b"echo oneterm_e2e\r");
    let found = wait_until(Duration::from_secs(6), || {
        snapshot_contains(&s, "oneterm_e2e")
    });
    let _ = s.close();
    assert!(
        found,
        "`echo oneterm_e2e` did not appear in the snapshot after 6s"
    );
}
