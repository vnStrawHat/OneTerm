//! A scrolled line costs no heap allocation once the history is full: the row
//! a trim drops lends its cells to the next row that is written.
//!
//! The counting allocator has to be a `#[global_allocator]`, which only an
//! integration test can install. It counts per thread, so the test harness's
//! own threads cannot pollute the figure and the test needs no `#[ignore]`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use oneterm_vt::{Config, EventBatch, Size, Terminal};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

fn count() {
    // `try_with`: an allocation made while the thread is being torn down has
    // no counter left to bump, and must not panic inside the allocator.
    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
}

fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn a_scrolled_line_allocates_nothing_once_the_history_is_full() {
    const LIMIT: u32 = 100;
    let mut term = Terminal::new(
        Size { rows: 24, cols: 80 },
        Config {
            scrollback_limit: LIMIT,
            ..Config::default()
        },
    );
    let mut batch = EventBatch::default();
    let now = Instant::now();
    // One line per feed, as a shell prints them: every feed scrolls once and
    // trims once.
    let lines: Vec<String> = (0..1_000).map(|n| format!("line {n}\r\n")).collect();

    // Fill the history past its limit, which also grows every reusable buffer
    // on the feed path to its high-water mark.
    for line in &lines {
        term.feed(line.as_bytes(), &mut batch, now);
    }
    assert_eq!(term.grid().primary().history_len(), LIMIT);

    let before = allocations();
    for line in &lines {
        term.feed(line.as_bytes(), &mut batch, now);
    }
    let made = allocations() - before;

    assert_eq!(
        made,
        0,
        "{} scrolled lines over a full history made {made} allocations",
        lines.len()
    );
    assert_eq!(term.grid().primary().history_len(), LIMIT);
}
