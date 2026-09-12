//! The two lines every adapter test needs: build an engine, feed it bytes.
//!
//! `oneterm_vt::testing` (the design's `terminal_from_text` / `feed`) is
//! `US-0082`'s row — the tests that would use it still run against the old
//! engine in this packet — so the adapter keeps its own pair. It is deliberately
//! not `mock_term`: sizing a grid to its content is a fixture convenience the
//! new tests do not need, and stating the size makes every assertion about a
//! row index unambiguous.

use std::time::Instant;

use oneterm_vt::EventBatch;

use crate::backend::GridSize;
use crate::engine::Engine;

/// A fresh engine at `size` with the default scrollback.
pub(crate) fn engine(size: GridSize) -> Engine {
    Engine::new(size, crate::engine::DEFAULT_SCROLLBACK_LINES)
}

/// Feed one chunk and drop the events.
pub(crate) fn feed(engine: &mut Engine, bytes: &[u8]) {
    let mut batch = EventBatch::new();
    engine.feed(bytes, &mut batch, Instant::now());
}
