//! "Bring your own transport": with `--no-default-features` the `pty` module is
//! gone and the engine still takes bytes from whatever the embedder owns.
//!
//! Adopted from the independent verification of `US-0104`, which built this
//! from outside the workspace against `default-features = false`. The half that
//! cannot live here is its companion: a binary naming `oneterm_vt::pty` must
//! *fail* to compile with the feature off. That is a build step, recorded in
//! the packet, not a test -- adding `trybuild` for one expectation would put a
//! dev-dependency in an embeddable crate to prove a `#[cfg]` works.

#![cfg(not(feature = "pty"))]

use std::time::Instant;

use oneterm_vt::{CellWidth, EventBatch, RenderContent, RenderRow, RenderState, Size, Terminal};

fn row_text(row: &RenderRow) -> String {
    let mut text = String::new();
    for cell in &row.cells {
        if cell.width == CellWidth::WideSpacer {
            continue;
        }
        match cell.content {
            RenderContent::Scalar(scalar) => text.push(scalar),
            RenderContent::Cluster { start, len } => text.extend(row.cluster(start, len)),
        }
    }
    text
}

#[test]
fn the_engine_feeds_and_renders_with_no_transport_compiled() {
    // The "transport" is a byte slice, which is the whole point.
    let bytes: &[u8] = b"\x1b[1;32mhello\x1b[0m no-pty";

    let mut terminal = Terminal::new(Size { rows: 4, cols: 32 }, Default::default());
    let mut batch = EventBatch::new();
    let stats = terminal.feed(bytes, &mut batch, Instant::now());
    assert_eq!(stats.bytes, bytes.len());

    let mut state = RenderState::new();
    let _ = terminal.render_update(&mut state, Instant::now());
    let first = state.rows().first().map(row_text).unwrap_or_default();
    assert!(
        first.starts_with("hello no-pty"),
        "the engine did not render the fed bytes: {first:?}"
    );
}
