//! Space pane-tree node types + pure tree-traversal helpers.
//!
//! A [`SpaceNode`] is either a leaf ([`SpaceLeaf`], holding a terminal view or
//! an empty placeholder) or a [`SpaceSplit`] (an axis + N child nodes sharing
//! one [`ResizableState`]). See `docs/terminal-split/01-architecture.md`.

use gpui::{Axis, Entity, FocusHandle};
use gpui_component::resizable::ResizableState;

use super::super::view::LocalTerminalView;

/// Stable identity for a Space leaf — used for active tracking, focus routing,
/// and drop targeting. Stable for the lifetime of the leaf.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SpaceId(pub u64);

/// One node of the Space tree.
pub enum SpaceNode {
    /// A leaf Space — holds a terminal or is empty.
    Leaf(SpaceLeaf),
    /// A split of two-or-more children along one axis.
    Split(SpaceSplit),
}

/// A leaf Space.
pub struct SpaceLeaf {
    pub id: SpaceId,
    pub content: SpaceContent,
    /// Focus target for the empty placeholder (the terminal leaf uses the
    /// view's own focus handle instead).
    pub focus: FocusHandle,
}

/// The content of a leaf Space.
pub enum SpaceContent {
    /// A live terminal view (local or SSH — both are `LocalTerminalView`).
    Terminal(Entity<LocalTerminalView>),
    /// Empty Space: renders placeholder text.
    Empty,
}

/// An internal split node: `children` laid out along `axis`, sharing `state`.
pub struct SpaceSplit {
    /// Horizontal split = children laid out left→right (Split Right/Left).
    /// Vertical split = children laid out top→bottom (Split Up/Down).
    pub axis: Axis,
    pub children: Vec<SpaceNode>,
    /// Sizes/handles for this split level (one entity per split node).
    pub state: Entity<ResizableState>,
}

impl SpaceNode {
    /// Find the leaf with `id` in this subtree.
    pub fn find_leaf(&self, id: SpaceId) -> Option<&SpaceLeaf> {
        match self {
            SpaceNode::Leaf(leaf) => (leaf.id == id).then_some(leaf),
            SpaceNode::Split(split) => split.children.iter().find_map(|c| c.find_leaf(id)),
        }
    }

    /// Find the leaf with `id` in this subtree (mutable).
    pub fn find_leaf_mut(&mut self, id: SpaceId) -> Option<&mut SpaceLeaf> {
        match self {
            SpaceNode::Leaf(leaf) => (leaf.id == id).then_some(leaf),
            SpaceNode::Split(split) => split.children.iter_mut().find_map(|c| c.find_leaf_mut(id)),
        }
    }

    /// Total number of leaf Spaces in this subtree.
    pub fn leaf_count(&self) -> usize {
        match self {
            SpaceNode::Leaf(_) => 1,
            SpaceNode::Split(split) => split.children.iter().map(|c| c.leaf_count()).sum(),
        }
    }

    /// The id of the first leaf in tree order.
    pub fn first_leaf_id(&self) -> SpaceId {
        match self {
            SpaceNode::Leaf(leaf) => leaf.id,
            // A well-formed split always has ≥ 1 child.
            SpaceNode::Split(split) => split.children[0].first_leaf_id(),
        }
    }

    /// The 0-based depth-first (left→right) index of leaf `id` in this subtree,
    /// or `None` if `id` is not a leaf here. Used for the Agent Panel's stable
    /// `#N` Space label (`docs/agent-panel-display.md` §5.1 / §14.1).
    pub fn leaf_index(&self, id: SpaceId) -> Option<usize> {
        fn walk(node: &SpaceNode, id: SpaceId, counter: &mut usize) -> Option<usize> {
            match node {
                SpaceNode::Leaf(leaf) => {
                    if leaf.id == id {
                        Some(*counter)
                    } else {
                        *counter += 1;
                        None
                    }
                }
                SpaceNode::Split(split) => {
                    for c in &split.children {
                        if let Some(i) = walk(c, id, counter) {
                            return Some(i);
                        }
                    }
                    None
                }
            }
        }
        let mut counter = 0;
        walk(self, id, &mut counter)
    }

    /// Collect every terminal view in this subtree.
    pub fn collect_terminal_views(&self, out: &mut Vec<Entity<LocalTerminalView>>) {
        match self {
            SpaceNode::Leaf(leaf) => {
                if let SpaceContent::Terminal(view) = &leaf.content {
                    out.push(view.clone());
                }
            }
            SpaceNode::Split(split) => {
                for c in &split.children {
                    c.collect_terminal_views(out);
                }
            }
        }
    }
}
