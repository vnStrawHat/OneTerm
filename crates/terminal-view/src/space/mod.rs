//! Terminal Split — the "Space" pane tree that lives inside a `TerminalPanel`.
//!
//! A `TerminalPanel` holds a [`SpaceTree`] instead of a single terminal view.
//! The tree's leaves are Spaces (a terminal or an empty placeholder); internal
//! nodes split the panel along an axis with resizable handles.
//!
//! - [`tree`] — ids, split context, nodes, and every tree transform
//! - [`render`] — the resizable layout, Space frames, empty placeholder, drag payload
//!
//! The current owning design is
//! `docs/spec-intakes/IN-0018-rebuild-terminal-render-engine/high-level-design.md`.

mod render;
#[cfg(test)]
mod tests;
mod tree;

pub(crate) use render::DragTerminalTab;
pub(crate) use tree::{
    CloseOutcome, SpaceContent, SpaceId, SpaceLeaf, SpaceTree, SplitContext, SplitDir,
};
