//! `BUG-0055`: what a discarded [`LocalSession`] leaves behind on Windows.
//!
//! The probe spawns the shipped default shell (`cmd.exe /K chcp 65001 >nul`),
//! drops the session at a parameterised delay after spawn, and then polls the
//! liveness of every process the spawn added as a child of this test process —
//! the `cmd.exe` client and the console host that serves its pseudo-console.
//!
//! It only ever looks at, and only ever terminates, processes whose parent is
//! this test process and which were not there before the spawn. No process is
//! matched by name for termination.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::time::{Duration, Instant};

use oneterm_terminal::TerminalRender;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess, WaitForSingleObject,
};

use super::session_tests::{lock_spawns, spawn_unguarded, wait_until};

/// The exit codes a watched process is allowed to end with.
///
/// `0x0` is the console host exiting with its last client. `0xc000013a` is
/// `STATUS_CONTROL_C_EXIT`, the host's close request processed normally.
/// `0x1` is OneTerm's own `TerminateProcess(handle, 1)` — the `DEC-0016`
/// escalation, and the only code that proves the escalation ran. `0xc0000142`
/// is `STATUS_DLL_INIT_FAILED`, the fault a client can take when its console
/// disappears mid-start. Anything else means the shell died of something this
/// packet did not measure.
const EXPECTED_EXIT_CODES: [u32; 4] = [0x0, 0x1, 0xc000_013a, 0xc000_0142];

/// How long a survivor is given before it counts as an orphan.
const LIVENESS_BOUND: Duration = Duration::from_secs(5);
/// Poll interval for the liveness sampling.
const POLL: Duration = Duration::from_millis(20);
/// Repetitions per delay — the baseline in the packet says the teardown is racy.
const REPETITIONS: usize = 5;

/// When the session is dropped, relative to its spawn.
#[derive(Clone, Copy)]
enum DropAfter {
    Millis(u64),
    FirstOutput,
}

impl DropAfter {
    fn label(self) -> String {
        match self {
            Self::Millis(ms) => format!("{ms} ms"),
            Self::FirstOutput => "first output".to_owned(),
        }
    }
}

/// A process this probe is responsible for: opened before the drop so the pid
/// cannot be recycled underneath the measurement.
struct Watched {
    pid: u32,
    name: String,
    handle: OwnedHandle,
}

impl Watched {
    fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }

    fn exited(&self) -> bool {
        // SAFETY: the handle is live and was opened with SYNCHRONIZE.
        unsafe { WaitForSingleObject(self.raw(), 0) == WAIT_OBJECT_0 }
    }

    fn exit_code(&self) -> Option<u32> {
        let mut code = 0u32;
        // SAFETY: the handle is live and opened with QUERY_LIMITED_INFORMATION.
        let read = unsafe { GetExitCodeProcess(self.raw(), &mut code) };
        (read != 0).then_some(code)
    }

    /// Last resort for a process this probe started and that refuses to exit.
    fn terminate(&self) {
        // SAFETY: the handle is live and was opened with PROCESS_TERMINATE.
        unsafe { TerminateProcess(self.raw(), 1) };
    }
}

/// Every live process whose parent is `parent`, as `(pid, image name)`.
fn children_of(parent: u32) -> Vec<(u32, String)> {
    let mut found = Vec::new();
    // SAFETY: a process snapshot takes no input buffer.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return found;
    }
    // SAFETY: the snapshot handle is owned here and closed exactly once.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot as _) };
    let raw = snapshot.as_raw_handle() as HANDLE;

    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    // SAFETY: `entry` is sized as the API requires and lives for both calls.
    let mut more = unsafe { Process32FirstW(raw, &mut entry) };
    while more != 0 {
        if entry.th32ParentProcessID == parent {
            let end = entry
                .szExeFile
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(entry.szExeFile.len());
            found.push((
                entry.th32ProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..end]),
            ));
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(raw, &mut entry) };
    }
    found
}

fn watch(pid: u32, name: String) -> Option<Watched> {
    // SAFETY: `pid` came from a snapshot; a failed open returns null.
    let handle = unsafe {
        OpenProcess(
            SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return None;
    }
    // SAFETY: `OpenProcess` succeeded, so the handle is owned here.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as _) };
    Some(Watched { pid, name, handle })
}

/// One row of the liveness table.
struct Row {
    name: String,
    pid: u32,
    exited_after: Option<Duration>,
    exit_code: Option<u32>,
    terminated: bool,
}

/// Spawn a default session, drop it after `delay`, and measure what survives.
fn probe(delay: DropAfter) -> Vec<Row> {
    let ours = std::process::id();

    // Held across both snapshots: a sibling test's shell starting in between
    // would be adopted here, waited out and terminated. Every spawn in this
    // binary takes the same lock (see `session_tests::SPAWN_GUARD`).
    let guard = lock_spawns();
    let before: Vec<u32> = children_of(ours).into_iter().map(|(pid, _)| pid).collect();
    let session = spawn_unguarded(oneterm_core::LocalShellConfig::default()).expect("spawn");
    let watched: Vec<Watched> = children_of(ours)
        .into_iter()
        .filter(|(pid, _)| !before.contains(pid))
        .filter_map(|(pid, name)| watch(pid, name))
        .collect();
    drop(guard);

    match delay {
        DropAfter::Millis(ms) => std::thread::sleep(Duration::from_millis(ms)),
        DropAfter::FirstOutput => {
            assert!(
                wait_until(Duration::from_secs(10), || !session
                    .snapshot()
                    .text()
                    .trim()
                    .is_empty()),
                "the shell produced no output within 10 s"
            );
        }
    }

    let dropped = Instant::now();
    drop(session);

    let mut rows: Vec<Row> = watched
        .iter()
        .map(|process| Row {
            name: process.name.clone(),
            pid: process.pid,
            exited_after: None,
            exit_code: None,
            terminated: false,
        })
        .collect();

    let deadline = dropped + LIVENESS_BOUND;
    loop {
        let mut pending = false;
        for (row, process) in rows.iter_mut().zip(watched.iter()) {
            if row.exited_after.is_some() {
                continue;
            }
            if process.exited() {
                row.exited_after = Some(dropped.elapsed());
                row.exit_code = process.exit_code();
            } else {
                pending = true;
            }
        }
        if !pending || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }

    // Cleanup obligation: only processes this probe started, only by pid.
    for (row, process) in rows.iter_mut().zip(watched.iter()) {
        if row.exited_after.is_none() {
            process.terminate();
            row.terminated = true;
        }
    }

    rows
}

/// Every process the spawn added must be gone within [`LIVENESS_BOUND`], and
/// gone for a reason this packet measured.
fn assert_no_orphan(delay: DropAfter, runs: usize) {
    for run in 1..=runs {
        let rows = probe(delay);
        assert!(
            !rows.is_empty(),
            "the spawn added no child process, so the probe measured nothing"
        );
        for row in rows {
            assert!(
                !row.terminated,
                "run {run}: dropping a session after {} left {} (pid {}) alive for \
                 {LIVENESS_BOUND:?}; the probe had to terminate it",
                delay.label(),
                row.name,
                row.pid,
            );
            let code = row
                .exit_code
                .expect("an exited process must have an exit code");
            assert!(
                EXPECTED_EXIT_CODES.contains(&code),
                "run {run}: {} (pid {}) exited with an unexpected {code:#x} after a drop at {}",
                row.name,
                row.pid,
                delay.label(),
            );
        }
    }
}

/// `BUG-0055`: the session is discarded while `cmd.exe` is still starting, so
/// the client never processes the exit request its host sends when the
/// pseudo-console closes. Without the bounded wait and escalation in
/// `ChildExitWatcher::drop` (`DEC-0016`) this leaves a `cmd.exe` and its console
/// host behind — measured, still alive 15 s later in 8 of 9 runs.
///
/// A green run is **not** proof that the escalation ran: on hardware where
/// `cmd.exe` finishes initialising before the spawn path returns, the orphan
/// never occurs and this passes vacuously. The exit codes say which happened —
/// `0x1` is the escalation, `0xc000013a` is the shell exiting on its own — and
/// the ignored `orphan_liveness_table` is the manual proof that prints them.
#[test]
fn a_session_dropped_before_its_shell_starts_leaves_no_orphan() {
    assert_no_orphan(DropAfter::Millis(0), REPETITIONS);
}

/// The case that already worked before `BUG-0055`, pinned so the fix cannot
/// regress it: a fully started shell exits on its own when the pseudo-console
/// closes, well inside the grace period.
#[test]
fn a_session_dropped_after_its_shell_is_up_leaves_no_orphan() {
    assert_no_orphan(DropAfter::FirstOutput, 1);
}

/// The `BUG-0055` diagnosis sweep, kept for the record. Ignored because it
/// starts a shell per row and waits out [`LIVENESS_BOUND`] for every survivor;
/// run it with
/// `cargo test -p oneterm-local-shell --lib -- --ignored --nocapture orphan_liveness_table`.
#[test]
#[ignore = "spawns real cmd.exe processes and can take minutes; run explicitly"]
fn orphan_liveness_table() {
    println!("| drop delay | run | process | pid | exited after | exit code | terminated |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for delay in [
        DropAfter::Millis(0),
        DropAfter::Millis(5),
        DropAfter::Millis(10),
        DropAfter::Millis(50),
        DropAfter::Millis(500),
        DropAfter::FirstOutput,
    ] {
        for run in 1..=REPETITIONS {
            for row in probe(delay) {
                println!(
                    "| {} | {run} | {} | {} | {} | {} | {} |",
                    delay.label(),
                    row.name,
                    row.pid,
                    row.exited_after
                        .map_or_else(|| "never".to_owned(), |at| format!("{} ms", at.as_millis())),
                    row.exit_code
                        .map_or_else(|| "-".to_owned(), |code| format!("{code:#x}")),
                    if row.terminated { "yes" } else { "no" },
                );
            }
        }
    }
}
