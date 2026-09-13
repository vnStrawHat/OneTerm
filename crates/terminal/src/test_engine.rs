//! The two lines every adapter test needs: build a terminal, feed it bytes.
//!
//! `US-0081`'s gap 5 filed a shared `oneterm_vt::testing` module here. It is
//! declined: `docs/agents/code-style.md` says to extract shared code once
//! multiple crates need it, and one does. Sizing a grid to its content — the
//! reference's `mock_term` convenience — is not reproduced either; stating the
//! size makes every assertion about a row index unambiguous, and the one suite
//! that wants it (`search`) keeps its own three-line builder.

use std::time::Instant;

use oneterm_vt::{EventBatch, Terminal};

use crate::backend::GridSize;

/// A fresh terminal at `size` with the default scrollback.
pub(crate) fn terminal(size: GridSize) -> Terminal {
    Terminal::new(
        size.into(),
        oneterm_vt::Config {
            scrollback_limit: crate::handle::DEFAULT_SCROLLBACK_LINES as u32,
            ..oneterm_vt::Config::default()
        },
    )
}

/// Feed one chunk and drop the events.
pub(crate) fn feed(term: &mut Terminal, bytes: &[u8]) {
    let mut batch = EventBatch::new();
    term.feed(bytes, &mut batch, Instant::now());
}
