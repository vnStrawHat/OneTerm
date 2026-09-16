//! Tests for `LocalSession`.

use std::time::{Duration, Instant};

use oneterm_terminal::SelectionKind as SelectionType;
use oneterm_terminal::{MouseModifiers, TerminalMouseButton};
use oneterm_terminal::{
    SessionKind, TerminalError, TerminalIme, TerminalInput, TerminalLifecycle, TerminalRender,
    TerminalSecurityPolicy,
};

use crate::session::{LocalSession, quote_windows_argument};
use oneterm_core::AppError;
use oneterm_terminal::{PtySession, PtySize};

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

pub(super) fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return true;
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
    predicate()
}

fn snapshot_contains(session: &PtySession<LocalSession>, needle: &str) -> bool {
    session.snapshot().text().contains(needle)
}

/// Ceiling for one round trip through a real shell: write, and wait for the
/// shell's answer to reach the snapshot or its exit to reach `alive()`.
///
/// This is a *detects-at-all* gate, not a latency budget. Every test that uses
/// it fails the same way whether the bound is one second or fifteen — the shell
/// never answered — so the number buys nothing but flake resistance, and the
/// tests here run 8-wide against a real shell each, on runners with two vCPUs.
///
/// Measured on the owner's host with the whole binary pinned to two logical
/// CPUs at `--test-threads=8` (ten runs, worst case of each wait): shell exit
/// 405 ms, `hello` echo 437 ms, `hello_world` echo 415 ms, `oneterm_e2e` echo
/// 169 ms. The bounds these replaced were 4 s, 2 s, 2 s and 6 s, so the tightest
/// of them had under 5x headroom — and a two-vCPU ubuntu runner blew the 4 s one
/// (BUG-0066). 15 s was already this file's bound for the PowerShell prompt.
const SHELL_ROUND_TRIP: Duration = Duration::from_secs(15);

/// The snapshot's non-blank lines, for an assertion message.
///
/// The grid persists, so this answers "did the shell ever print anything (a
/// prompt, an error) at all" — which is what separates a shell that never
/// started from one that ran and whose exit never came back.
fn snapshot_lines(session: &PtySession<LocalSession>) -> Vec<String> {
    session
        .snapshot()
        .text()
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Serialises every real-shell spawn in this test binary.
///
/// `session_orphan_tests` works out which processes belong to its session by
/// diffing this process's children around the spawn. Another test's shell
/// starting inside that window would be adopted by the probe, waited out for
/// `LIVENESS_BOUND` and then terminated — failing the probe and very likely the
/// innocent test too. So every spawn here takes this lock, and the probe holds
/// it across both of its snapshots.
pub(super) static SPAWN_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(super) fn lock_spawns() -> std::sync::MutexGuard<'static, ()> {
    SPAWN_GUARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Spawn `cfg` while holding [`SPAWN_GUARD`].
fn spawn_guarded(
    cfg: oneterm_core::LocalShellConfig,
) -> Result<PtySession<LocalSession>, AppError> {
    let _guard = lock_spawns();
    spawn_unguarded(cfg)
}

/// Spawn `cfg`. The caller must already hold [`SPAWN_GUARD`].
pub(super) fn spawn_unguarded(
    cfg: oneterm_core::LocalShellConfig,
) -> Result<PtySession<LocalSession>, AppError> {
    LocalSession::spawn(
        cfg,
        PtySize { rows: 24, cols: 80 },
        10_000,
        TerminalSecurityPolicy::default(),
        oneterm_core::TerminalLogConfig::default(),
    )
}

pub(super) fn spawn_default() -> PtySession<LocalSession> {
    spawn_guarded(oneterm_core::LocalShellConfig::default()).expect("spawn")
}

#[cfg(windows)]
fn assert_powershell_prompt_emits_cwd(kind: oneterm_core::ShellKind, label: &str) {
    let cfg = oneterm_core::LocalShellConfig {
        kind,
        ..Default::default()
    };
    let session = spawn_guarded(cfg).unwrap_or_else(|error| panic!("spawn {label}: {error}"));

    let emitted_cwd = wait_until(SHELL_ROUND_TRIP, || session.cwd().is_some());
    // `snapshot()` consumes render damage, which is fine here: no renderer runs.
    let snapshot = session.snapshot().text();
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

/// DEC-0008: a ConPTY session must not pull scrollback into a grown viewport.
#[test]
fn local_session_grow_policy_matches_conpty() {
    let session = spawn_default();
    let expected = if cfg!(windows) {
        oneterm_terminal::ResizePolicy::KeepViewportTop
    } else {
        oneterm_terminal::ResizePolicy::BottomAnchor
    };
    assert_eq!(session.resize_policy(), expected);
}

#[test]
fn trait_snapshot_bounds() {
    let s = spawn_default();
    let snap = s.snapshot();
    assert_eq!(snap.size().cols, 80);
    assert_eq!(snap.size().rows, 24);
    let _ = s.close();
}

#[test]
fn trait_alive_is_local_close() {
    let s = spawn_default();
    assert!(s.alive());
    assert_eq!(s.kind(), SessionKind::Local);
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
    let guard = s.term().lock();
    let started = Instant::now();
    s.close().expect("close must succeed");
    let elapsed = started.elapsed();
    drop(guard);
    assert!(
        elapsed < Duration::from_millis(500),
        "close() blocked for {elapsed:?} — it must hand the owner thread to the reaper"
    );
    assert!(
        s.owner().owner_join.lock().unwrap().is_none(),
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
    let error = spawn_guarded(cfg)
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
    assert_eq!(s.snapshot().size().cols, 100);
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
    assert!(
        wait_until(SHELL_ROUND_TRIP, || snapshot_contains(&s, "hello")),
        "`hello` never reached the snapshot; blank lines dropped: {:?}",
        snapshot_lines(&s)
    );
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
    assert!(
        wait_until(SHELL_ROUND_TRIP, || {
            snapshot_contains(&s, "hello_world")
        }),
        "`hello_world` never reached the snapshot; blank lines dropped: {:?}",
        snapshot_lines(&s)
    );
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
        snap.selection_range().is_none(),
        "mouse_move should not update selection"
    );
    // mouse_drag should update selection
    s.mouse_drag(0.0, 5.0, MouseModifiers::default());
    let snap2 = s.snapshot();
    assert!(
        snap2.selection_range().is_some(),
        "mouse_drag should update selection"
    );
    if let Some(sel) = snap2.selection_range() {
        assert_eq!(sel.start.col, 0);
        assert!(
            sel.end.col >= 4,
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
fn spawned_shell_exit_is_detected() {
    // Cross-platform round trip: request shell exit and observe lifecycle state.
    let s = spawn_default();
    let started = Instant::now();
    let _ = s.write(b"exit\r");
    let detected = wait_until(SHELL_ROUND_TRIP, || !s.alive());
    assert!(
        detected,
        "shell exit not detected in {:?} (bound {SHELL_ROUND_TRIP:?}); alive={} at the \
         end; terminal snapshot, blank lines dropped: {:?}",
        started.elapsed(),
        s.alive(),
        snapshot_lines(&s),
    );
}

/// End-to-end: spawn the platform shell, write `echo oneterm_e2e`, and assert
/// that the text reaches the terminal snapshot without requiring a GUI.
#[test]
fn e2e_echo_output_rendered_in_snapshot() {
    let s = spawn_default();
    let _ = s.write(b"echo oneterm_e2e\r");
    let found = wait_until(SHELL_ROUND_TRIP, || snapshot_contains(&s, "oneterm_e2e"));
    let lines = snapshot_lines(&s);
    let _ = s.close();
    assert!(
        found,
        "`echo oneterm_e2e` never appeared in the snapshot; blank lines dropped: {lines:?}"
    );
}
