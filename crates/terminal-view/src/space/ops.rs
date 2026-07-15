//! Tree-mutating operations on [`SpaceTree`]: split, close (+ collapse),
//! fill-empty, and take-terminal. These are pure with respect to GPUI — leaf
//! focus handles and split `ResizableState`s are pre-created by the caller
//! (the panel) and passed in, so this module needs no `Context`.
//!
//! See `docs/terminal-split/02-split-and-close.md`.

use gpui::{Axis, Entity};
use gpui_component::resizable::ResizableState;

use super::super::view::LocalTerminalView;
use super::node::{SpaceContent, SpaceId, SpaceLeaf, SpaceNode, SpaceSplit};
use super::{CloseOutcome, SpaceTree, SplitDir};

impl SpaceTree {
    /// Split leaf `target` in `dir`, inserting `empty` (a pre-built empty leaf)
    /// as a new sibling. `state` is the `ResizableState` for the new split node.
    /// The new empty leaf becomes active.
    pub fn split(
        &mut self,
        target: SpaceId,
        dir: SplitDir,
        empty: SpaceLeaf,
        state: Entity<ResizableState>,
    ) {
        let new_id = empty.id;
        let axis = dir.axis();
        let after = dir.new_after();

        let mut empty = Some(empty);
        let mut state = Some(state);
        let root = self.take_root();
        self.set_root(split_transform(
            root, target, axis, after, &mut empty, &mut state,
        ));

        // `empty` is consumed only if the target leaf was found.
        if empty.is_none() {
            self.active = new_id;
        }
    }

    /// Close leaf `target`, collapsing the tree to keep it well-formed. Returns
    /// the removed terminal view (if the leaf held one) so the caller can close
    /// its session, plus the [`CloseOutcome`].
    pub fn close(&mut self, target: SpaceId) -> (CloseOutcome, Option<Entity<LocalTerminalView>>) {
        if self.leaf_count() <= 1 {
            return (CloseOutcome::LastSpaceClosed, None);
        }

        let mut removed: Option<SpaceContent> = None;
        let root = self.take_root();
        match close_transform(root, target, &mut removed) {
            Some(new_root) => self.set_root(new_root),
            None => unreachable!("close guarded by leaf_count > 1"),
        }

        // Pick a new active leaf if the active one was removed.
        if !self.has_leaf(self.active) {
            self.active = self.cur().first_leaf_id();
        }

        let view = match removed {
            Some(SpaceContent::Terminal(view)) => Some(view),
            _ => None,
        };
        (CloseOutcome::Removed, view)
    }

    /// Replace the empty content of leaf `target` with a terminal `view`.
    /// No-op if `target` is not an empty leaf. Activates the filled leaf.
    pub fn fill_empty(&mut self, target: SpaceId, view: Entity<LocalTerminalView>) {
        if let Some(leaf) = self.cur_mut().find_leaf_mut(target) {
            if matches!(leaf.content, SpaceContent::Empty) {
                leaf.content = SpaceContent::Terminal(view);
                self.active = target;
            }
        }
    }

    /// Take the terminal view out of leaf `id`, leaving the leaf empty. Returns
    /// the removed view, or `None` if the leaf was already empty / missing.
    pub fn take_leaf_terminal(&mut self, id: SpaceId) -> Option<Entity<LocalTerminalView>> {
        let leaf = self.cur_mut().find_leaf_mut(id)?;
        match std::mem::replace(&mut leaf.content, SpaceContent::Empty) {
            SpaceContent::Terminal(view) => Some(view),
            SpaceContent::Empty => None,
        }
    }
}

/// Owned recursive transform for `split`: find `target` and wrap it in a new
/// `Split`, consuming `empty`/`state` exactly once.
fn split_transform(
    node: SpaceNode,
    target: SpaceId,
    axis: Axis,
    after: bool,
    empty: &mut Option<SpaceLeaf>,
    state: &mut Option<Entity<ResizableState>>,
) -> SpaceNode {
    match node {
        SpaceNode::Leaf(leaf) => {
            if leaf.id == target {
                let new_leaf = SpaceNode::Leaf(empty.take().expect("empty leaf present"));
                let existing = SpaceNode::Leaf(leaf);
                let children = if after {
                    vec![existing, new_leaf]
                } else {
                    vec![new_leaf, existing]
                };
                SpaceNode::Split(SpaceSplit {
                    axis,
                    children,
                    state: state.take().expect("state present"),
                })
            } else {
                SpaceNode::Leaf(leaf)
            }
        }
        SpaceNode::Split(split) => {
            let SpaceSplit {
                axis: a,
                children,
                state: s,
            } = split;
            let mut new_children = Vec::with_capacity(children.len());
            for child in children {
                if empty.is_some() {
                    new_children.push(split_transform(child, target, axis, after, empty, state));
                } else {
                    new_children.push(child);
                }
            }
            SpaceNode::Split(SpaceSplit {
                axis: a,
                children: new_children,
                state: s,
            })
        }
    }
}

/// Owned recursive transform for `close`: remove `target`, capturing its content
/// into `removed`, and collapse any split left with a single child.
/// Returns `None` if this node itself should be removed by its parent.
fn close_transform(
    node: SpaceNode,
    target: SpaceId,
    removed: &mut Option<SpaceContent>,
) -> Option<SpaceNode> {
    match node {
        SpaceNode::Leaf(leaf) => {
            if leaf.id == target {
                *removed = Some(leaf.content);
                None
            } else {
                Some(SpaceNode::Leaf(leaf))
            }
        }
        SpaceNode::Split(split) => {
            let SpaceSplit {
                axis,
                children,
                state,
            } = split;
            let mut new_children = Vec::with_capacity(children.len());
            for child in children {
                if removed.is_none() {
                    if let Some(n) = close_transform(child, target, removed) {
                        new_children.push(n);
                    }
                } else {
                    new_children.push(child);
                }
            }
            match new_children.len() {
                0 => None,
                1 => Some(new_children.into_iter().next().unwrap()),
                _ => Some(SpaceNode::Split(SpaceSplit {
                    axis,
                    children: new_children,
                    state,
                })),
            }
        }
    }
}
