//! The `pty` module as an **embedder** sees it: an integration test, so nothing
//! here can reach a private item. Adopted from the independent verification of
//! `US-0104`, which ran these against the crate from outside the workspace.
//!
//! Process hygiene, and it is not optional: every child is tracked by the pid
//! [`PseudoConsole::child_pid`] returned, and liveness is asked of that pid
//! alone. Nothing is ever matched, enumerated or terminated by image name --
//! this machine runs other consoles, including the one you are reading this in.
//!
//! Windows only, because it spawns `cmd.exe`. The Unix `openpty` path has no
//! process-spawning test; that gap predates the move.

#![cfg(all(windows, feature = "pty"))]

use std::io::Read;
use std::time::{Duration, Instant};

use oneterm_vt::pty::{
    ChildEvent, EventedPty, EventedReadWrite, OnResize, Options, PseudoConsole, Shell, WindowSize,
};

fn size(rows: u16, cols: u16) -> WindowSize {
    WindowSize {
        rows,
        cols,
        cell_width: 0,
        cell_height: 0,
    }
}

/// `Options` must be constructible from `Default` without naming a cfg-gated
/// field. If `escape_args` or `child_signal_mask` had to be spelled out,
/// portable embedder code would be impossible to write.
fn options(args: &[&str]) -> Options {
    Options {
        shell: Some(Shell::new(
            "cmd.exe".to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        )),
        ..Options::default()
    }
}

/// Is that pid still a live process? Filtered by pid, never by name.
fn alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist.exe")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

fn read_until(console: &mut PseudoConsole, needle: &[u8], timeout: Duration) -> Vec<u8> {
    let deadline = Instant::now() + timeout;
    let mut output = Vec::new();
    while Instant::now() < deadline {
        let mut buffer = [0u8; 4096];
        match console.reader().read(&mut buffer) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)),
            Ok(read) => output.extend_from_slice(&buffer[..read]),
            Err(_) => std::thread::sleep(Duration::from_millis(10)),
        }
        if output.windows(needle.len()).any(|window| window == needle) {
            return output;
        }
    }
    output
}

/// A real child through the public API only: it runs, its bytes come back, its
/// exit is reported without a read, and its pid is gone once the console drops.
#[test]
fn a_public_api_child_runs_reports_and_leaves_nothing_behind() {
    let mut console =
        PseudoConsole::spawn(&options(&["/c", "echo", "vt-pty-contract"]), size(24, 80))
            .expect("spawn");
    // No liveness assert here: `cmd /c echo` can be gone before `tasklist`
    // answers. The pid is captured for the post-drop check below.
    let pid = console.child_pid().expect("a child pid");

    let output = read_until(&mut console, b"vt-pty-contract", Duration::from_secs(15));
    assert!(
        output
            .windows(15)
            .any(|window| window == b"vt-pty-contract"),
        "the marker never arrived: {}",
        String::from_utf8_lossy(&output)
    );

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut exit = None;
    while Instant::now() < deadline {
        if let Some(ChildEvent::Exited(status)) = console.next_child_event() {
            exit = Some(status);
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    exit.expect("ChildEvent::Exited never arrived");

    drop(console);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !alive(pid),
        "the child pid {pid} outlived its pseudo-console"
    );
}

/// Resize on both sides of the child's exit. Afterwards the console is already
/// gone, so an error is legitimate; a panic is not, which is what `OnResize`'s
/// `io::Result` return promises.
#[test]
fn resize_is_safe_on_both_sides_of_the_child_exit() {
    let mut console =
        PseudoConsole::spawn(&options(&["/c", "echo", "vt-pty-resize"]), size(24, 80))
            .expect("spawn");
    let pid = console.child_pid().expect("a child pid");

    console
        .on_resize(size(30, 100))
        .expect("resize while alive");

    let _ = read_until(&mut console, b"vt-pty-resize", Duration::from_secs(15));
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if console.next_child_event().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let after = console.on_resize(size(10, 40));
    println!("resize after the child exit -> {after:?}");

    drop(console);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !alive(pid),
        "the child pid {pid} outlived its pseudo-console"
    );
}

/// Dropping a pseudo-console whose child is still running ends that child,
/// inside the documented two-second grace, and only that child.
#[test]
fn dropping_a_live_console_ends_its_child_within_the_grace() {
    // `ping -n 30` keeps cmd.exe busy in the foreground for about 30 seconds.
    let mut console = PseudoConsole::spawn(
        &options(&["/c", "ping", "-n", "30", "127.0.0.1"]),
        size(24, 80),
    )
    .expect("spawn");
    let pid = console.child_pid().expect("a child pid");

    // Wait for the first byte, so the child has finished starting. Any output
    // will do: no locale assumption.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let mut buffer = [0u8; 4096];
        match console.reader().read(&mut buffer) {
            Ok(read) if read > 0 => break,
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    assert!(alive(pid), "the child was not running before the drop");

    let started = Instant::now();
    drop(console);
    let elapsed = started.elapsed();

    // Close the console, wait a bounded grace, then terminate.
    assert!(
        elapsed < Duration::from_secs(4),
        "the drop blocked for {elapsed:?}, past the documented two-second grace"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !alive(pid),
        "the child pid {pid} survived the drop of its pseudo-console"
    );
}
