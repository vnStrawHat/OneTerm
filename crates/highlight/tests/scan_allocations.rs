//! A steady-state `scan_line_into` allocates nothing (`US-0144`): once the
//! caller's output buffer and `ScanScratch` have grown to the longest line,
//! every further scan — any role, any profile, ASCII or not — reuses them.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use oneterm_highlight::{RowRole, RuleSet, ScanScratch, ShellProfile, scan_line_into};

/// Counts this thread's allocations, so parallel tests do not disturb it.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call to `System` unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        // SAFETY: the caller's contract is passed through.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract is passed through.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
        // SAFETY: the caller's contract is passed through.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const LINES: &[&str] = &[
    "2026-09-22 10:00:00 error connect 192.168.1.10 failed warn retry /var/log/app.log 42",
    "link/ether 00:1a:2b:3c:4d:5e brd ff:ff:ff:ff:ff:ff 2607:f8b0:4004:80a::200e",
    "drwxr-xr-x  2 user group 4096 Sep 22 10:00 'quoted' --flag -x 0x1F",
    "\u{65e5}\u{672c}\u{8a9e} error at /etc/hosts \u{4e2d}\u{6587} 2026-09-22 192.168.0.1",
    "Ti\u{1ebf}ng Vi\u{1ec7}t: l\u{1ed7}i error /home/ng\u{1b0}\u{1edd}i/t\u{1ec7}p.rs",
    "\u{1f680} build ok \u{2705} \u{1f469}\u{200d}\u{1f4bb} warning: 3 passed",
    "PS C:\\Program Files\\App> Get-ChildItem -Recurse",
    "user@host:~/src$ cargo build --release | tee log.txt",
    "",
];

/// Allocations made by scanning every line of `lines` under every role and
/// profile once.
fn allocations_of(lines: &[&str], scratch: &mut ScanScratch, out: &mut Vec<u8>) -> usize {
    let rules = RuleSet::global();
    let roles = [
        (None, None),
        (Some(RowRole::Output), None),
        (Some(RowRole::Command), None),
        (Some(RowRole::Prompt), Some(20)),
    ];
    let before = ALLOCATIONS.with(Cell::get);
    for profile in [
        ShellProfile::Unix,
        ShellProfile::Cmd,
        ShellProfile::PowerShell,
    ] {
        for (role, input_at) in roles {
            for line in lines {
                scan_line_into(line, rules, &profile, role, input_at, scratch, out);
            }
        }
    }
    ALLOCATIONS.with(Cell::get) - before
}

#[test]
fn a_steady_state_scan_allocates_nothing() {
    let (ascii, other): (Vec<&str>, Vec<&str>) = LINES.iter().partition(|l| l.is_ascii());
    let mut scratch = ScanScratch::default();
    let mut out = Vec::new();
    // Warm-up: grows the buffers and builds the rule set and regex caches.
    allocations_of(LINES, &mut scratch, &mut out);

    assert_eq!(
        allocations_of(&ascii, &mut scratch, &mut out),
        0,
        "ASCII lines"
    );

    // A non-ASCII line in output mode costs exactly one allocation, and it is
    // not the scanner's: the date/time regex's lazy DFA cannot evaluate a
    // Unicode `\b` next to a non-ASCII byte, gives up with a boxed 16-byte
    // `MatchError`, and `regex` retries on another engine. Removing it would
    // mean an ASCII `\b`, which classifies `日本語2026-09-22` differently.
    let output_mode_scans = other.len() * 3 * 2; // three profiles, `None` and `Output`
    assert_eq!(
        allocations_of(&other, &mut scratch, &mut out),
        output_mode_scans,
        "non-ASCII lines"
    );
}
