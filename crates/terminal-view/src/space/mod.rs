//! Terminal Split — the "Space" pane tree that lives inside a `TerminalPanel`.
//!
//! A `TerminalPanel` holds a [`SpaceTree`] instead of a single terminal view.
//! The tree's leaves are Spaces (a terminal or an empty placeholder); internal
//! nodes split the panel along an axis with resizable handles. See the design
//! in `docs/terminal-split/`.

mod drag;
mod node;
pub(crate) mod ops;
pub(crate) mod placeholder;
pub(crate) mod render;
#[cfg(test)]
mod tests;
mod tree;

pub(crate) use node::{SpaceContent, SpaceId, SpaceLeaf};
pub(crate) use render::render_node;
pub(crate) use tree::{CloseOutcome, SpaceTree, SplitContext, SplitDir};

pub use drag::DragTerminalTab;
