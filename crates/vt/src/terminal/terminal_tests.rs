//! Byte-feed tests for the dispatch layer.
//!
//! Every test drives real bytes through a real [`Terminal`] and asserts on
//! cells, the cursor, modes or the event batch — the shape
//! `testing-and-bench.md` § 1 names.

use std::time::Instant;

use super::*;
use crate::grid::Size;

fn terminal(cols: u16, rows: u16) -> Terminal {
    Terminal::new(Size { rows, cols }, Config::default())
}

fn feed(term: &mut Terminal, bytes: &[u8]) -> EventBatch {
    let mut batch = EventBatch::new();
    term.feed(bytes, &mut batch, Instant::now());
    batch
}

#[test]
fn a_terminal_prints_and_wraps() {
    let mut term = terminal(4, 2);
    feed(&mut term, b"abcdef");

    let top = term.viewport().top;
    assert_eq!(term.row_text(top), "abcd");
    assert_eq!(term.row_text(top + 1), "ef  ");
}
