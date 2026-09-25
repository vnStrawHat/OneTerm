//! A steady-state `scan_line_into` allocates nothing of its own (`US-0144`):
//! once the caller's output buffer and `ScanScratch` have grown to the longest
//! line, every further scan reuses them. An ASCII line allocates nothing at
//! all; a non-ASCII line may still see `regex`'s own small allocations, which
//! are not the scanner's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use oneterm_highlight::{RowRole, RuleSet, ScanScratch, ShellProfile, scan_line_into};

/// Counts this thread's allocations, so parallel tests do not disturb it.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

fn count(size: usize) {
    ALLOCATIONS.with(|n| n.set(n.get() + 1));
    LARGEST.with(|l| l.set(l.get().max(size)));
}

// SAFETY: forwards every call to `System` unchanged; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller's contract is passed through.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract is passed through.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
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
    "\u{65e5}\u{672c}\u{8a9e}2026-09-22 caf\u{e9}Mon \u{e9}00:1a:2b:3c:4d:5e 2026-09-22 10:00 \u{65e5}",
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

    // A non-ASCII line can still allocate, and not in the scanner: `regex`
    // boxes a 16-byte `MatchError` whenever its lazy DFA gives up on a Unicode
    // `\b` next to a non-ASCII byte (the date/time and MAC patterns) and it
    // retries on another engine — one per such search, so the count depends on
    // the line and on the `regex` version. What this crate controls is that no
    // per-line buffer is allocated: every line here is at least 20 chars, so
    // the smallest buffer the scanner could allocate (`chars`, 4 B per char)
    // is 80 B, far above `regex`'s box. Keeping the Unicode `\b` is decided:
    // `(?-u:\b)` would reclassify `日本語2026-09-22` (see the golden corpus).
    assert!(other.iter().all(|l| l.chars().count() >= 20));
    LARGEST.with(|l| l.set(0));
    allocations_of(&other, &mut scratch, &mut out);
    let largest = LARGEST.with(Cell::get);
    assert!(
        largest <= 32,
        "a non-ASCII scan allocated {largest} bytes at once: a per-line buffer is back"
    );
}
