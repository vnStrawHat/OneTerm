//! End-to-end checks against a real ConPTY child.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use super::*;
use crate::pty::Shell;

fn size(rows: u16, cols: u16) -> WindowSize {
    WindowSize {
        rows,
        cols,
        cell_width: 0,
        cell_height: 0,
    }
}

fn spawn(args: &[&str]) -> PseudoConsole {
    let options = Options {
        shell: Some(Shell::new(
            "cmd.exe".to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect(),
        )),
        ..Options::default()
    };
    PseudoConsole::spawn(&options, size(24, 80)).expect("spawn a pseudo-console")
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
            break;
        }
    }
    output
}

#[test]
fn the_child_writes_through_the_pseudo_console() {
    let mut console = spawn(&["/c", "echo", "vt-pty-ok"]);
    let output = read_until(&mut console, b"vt-pty-ok", Duration::from_secs(10));
    assert!(
        output.windows(8).any(|window| window == b"vt-pty-o"),
        "the child's output never arrived: {}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn typed_input_reaches_the_child() {
    let mut console = spawn(&["/q", "/k", "echo off"]);
    console
        .writer()
        .write_all(b"echo vt-pty-typed\r\n")
        .expect("write input");
    let output = read_until(&mut console, b"vt-pty-typed", Duration::from_secs(10));
    assert!(
        output.windows(12).any(|window| window == b"vt-pty-typed"),
        "the typed command never echoed: {}",
        String::from_utf8_lossy(&output)
    );
}

#[test]
fn child_exit_is_reported_with_its_code() {
    let mut console = spawn(&["/c", "exit 3"]);

    let deadline = Instant::now() + Duration::from_secs(10);
    let event = loop {
        if let Some(event) = console.next_child_event() {
            break event;
        }
        assert!(
            Instant::now() < deadline,
            "the child exit was never reported"
        );
        std::thread::sleep(Duration::from_millis(10));
    };

    let ChildEvent::Exited(Some(status)) = event else {
        panic!("expected an exit status, got {event:?}");
    };
    assert_eq!(status.code(), Some(3));
    assert!(console.next_child_event().is_none());
}

#[test]
fn the_child_pid_is_available() {
    let console = spawn(&["/c", "exit 0"]);
    assert!(console.child_pid().is_some_and(|pid| pid > 0));
}

#[test]
fn a_rejected_resize_is_an_error_not_a_panic() {
    let mut console = spawn(&["/q", "/k", "echo off"]);
    // A degenerate size is the resize a host may reject. Whatever it answers,
    // the answer is a value: the implementation this replaces asserted on the
    // HRESULT and took the process down with it.
    if let Err(error) = console.on_resize(size(0, 0)) {
        assert!(error.to_string().contains("ResizePseudoConsole"), "{error}");
    }
    console.on_resize(size(40, 120)).expect("a valid resize");
}

/// `ClosePseudoConsole` blocks until the conout pipe is drained, so the
/// pseudo-console has to be the first field of `PseudoConsole`. If the fields
/// are ever reordered, this hangs.
#[test]
fn drop_order_drains_the_output_pipe() {
    let console = spawn(&["/c", "dir", "/s", "C:\\Windows\\System32\\drivers"]);
    // Never read a byte: the output sits in the pipe and the ring.
    std::thread::sleep(Duration::from_millis(300));

    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("oneterm-vt-pty-drop-order".to_owned())
        .spawn(move || {
            drop(console);
            let _ = done_tx.send(());
        })
        .expect("spawn the dropping thread");

    assert!(
        done_rx.recv_timeout(Duration::from_secs(20)).is_ok(),
        "dropping a pseudo-console with unread output did not finish"
    );
}
