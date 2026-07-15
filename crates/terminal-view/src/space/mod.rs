//! Terminal Split — the "Space" pane tree that lives inside a `TerminalPanel`.
//!
//! A `TerminalPanel` holds a [`SpaceTree`] instead of a single terminal view.
//! The tree's leaves are Spaces (a terminal or an empty placeholder); internal
//! nodes split the panel along an axis with resizable handles. See the design
//! in `docs/terminal-split/`.

use gpui::{Axis, Entity, FocusHandle, WeakEntity};

use super::view::LocalTerminalView;

mod node;
pub(crate) mod ops;
pub(crate) mod placeholder;
pub(crate) mod render;
#[cfg(test)]
mod tests;

pub(crate) use node::{SpaceContent, SpaceId, SpaceLeaf, SpaceNode};
pub(crate) use render::render_node;

pub use drag::DragTerminalTab;

mod drag;

/// Direction to split a Space in.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SplitDir {
    Right,
    Left,
    Up,
    Down,
}

impl SplitDir {
    /// The layout axis for this direction.
    pub fn axis(self) -> Axis {
        match self {
            SplitDir::Right | SplitDir::Left => Axis::Horizontal,
            SplitDir::Up | SplitDir::Down => Axis::Vertical,
        }
    }

    /// Whether the new (empty) child is inserted after the existing one.
    pub fn new_after(self) -> bool {
        matches!(self, SplitDir::Right | SplitDir::Down)
    }
}

/// Result of closing a Space.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CloseOutcome {
    /// A Space was removed; the tab still has ≥ 1 Space.
    Removed,
    /// The closed Space was the last one → the whole tab should close.
    LastSpaceClosed,
}

/// Context threaded into a terminal's context menu so its Split / Close-Space
/// items can target the right Space in the right panel. Cloned onto each
/// terminal view via [`LocalTerminalView`]'s `split_ctx` field.
#[derive(Clone)]
pub struct SplitContext {
    /// The panel owning the Space tree this terminal lives in.
    pub panel: WeakEntity<super::panel::TerminalPanel>,
    /// The Space (leaf) this terminal occupies.
    pub space_id: SpaceId,
}

/// The pane tree inside a `TerminalPanel`.
///
/// `root` is stored as an `Option` purely so tree-mutating ops can `take()` the
/// owned tree, transform it, and put it back — avoiding a `mem::replace`
/// sentinel that would need a focus handle. It is always `Some` between ops.
pub struct SpaceTree {
    root: Option<SpaceNode>,
    /// `SpaceId` allocator.
    next_id: u64,
    /// The active (focused) leaf.
    active: SpaceId,
}

impl SpaceTree {
    /// Create a tree with a single empty leaf (no terminal session).
    pub fn new_empty(focus: FocusHandle) -> Self {
        let id = SpaceId(0);
        let root = SpaceNode::Leaf(SpaceLeaf {
            id,
            content: SpaceContent::Empty,
            focus,
        });
        Self {
            root: Some(root),
            next_id: 1,
            active: id,
        }
    }

    /// Create a tree with a single terminal leaf wrapping `view`.
    pub fn new_terminal(view: Entity<LocalTerminalView>, focus: FocusHandle) -> Self {
        let id = SpaceId(0);
        let root = SpaceNode::Leaf(SpaceLeaf {
            id,
            content: SpaceContent::Terminal(view),
            focus,
        });
        Self {
            root: Some(root),
            next_id: 1,
            active: id,
        }
    }

    /// Borrow the root node (always present between operations).
    pub(crate) fn cur(&self) -> &SpaceNode {
        self.root.as_ref().expect("SpaceTree root present")
    }

    /// Mutably borrow the root node.
    pub(crate) fn cur_mut(&mut self) -> &mut SpaceNode {
        self.root.as_mut().expect("SpaceTree root present")
    }

    /// Take the owned root out (leaving `None`); callers must put a root back.
    pub(crate) fn take_root(&mut self) -> SpaceNode {
        self.root.take().expect("SpaceTree root present")
    }

    /// Put an owned root back.
    pub(crate) fn set_root(&mut self, root: SpaceNode) {
        self.root = Some(root);
    }

    /// Allocate a fresh, unused `SpaceId`.
    pub fn alloc_id(&mut self) -> SpaceId {
        let id = SpaceId(self.next_id);
        self.next_id += 1;
        id
    }

    /// The root node (for rendering).
    pub fn root(&self) -> &SpaceNode {
        self.cur()
    }

    /// Whether the tree is a single leaf (no splits) — the "plain terminal"
    /// fast path with no Space chrome.
    pub fn is_single(&self) -> bool {
        matches!(self.cur(), SpaceNode::Leaf(_))
    }

    /// The number of leaf Spaces.
    pub fn leaf_count(&self) -> usize {
        self.cur().leaf_count()
    }

    /// The active leaf id.
    pub fn active(&self) -> SpaceId {
        self.active
    }

    /// Set the active leaf id (no-op if the id is not a leaf).
    pub fn set_active(&mut self, id: SpaceId) {
        if self.cur().find_leaf(id).is_some() {
            self.active = id;
        }
    }

    /// Whether `id` is currently a leaf in the tree.
    pub fn has_leaf(&self, id: SpaceId) -> bool {
        self.cur().find_leaf(id).is_some()
    }

    /// The active leaf's terminal view, if the active Space holds one.
    pub fn active_terminal(&self) -> Option<Entity<LocalTerminalView>> {
        self.leaf_terminal(self.active)
    }

    /// The terminal view held by leaf `id`, if any.
    pub fn leaf_terminal(&self, id: SpaceId) -> Option<Entity<LocalTerminalView>> {
        match &self.cur().find_leaf(id)?.content {
            SpaceContent::Terminal(view) => Some(view.clone()),
            SpaceContent::Empty => None,
        }
    }

    /// Every terminal view in the tree (used to (re)subscribe to title events).
    pub fn terminal_views(&self) -> Vec<Entity<LocalTerminalView>> {
        let mut out = Vec::new();
        self.cur().collect_terminal_views(&mut out);
        out
    }

    /// Whether the tree contains no terminal leaves (all empty).
    pub fn has_no_terminals(&self) -> bool {
        self.terminal_views().is_empty()
    }

    /// The focus handle for the active leaf: the terminal view's handle for a
    /// terminal leaf, or the placeholder's handle for an empty leaf.
    pub fn active_focus_handle(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let leaf = self.cur().find_leaf(self.active)?;
        Some(match &leaf.content {
            SpaceContent::Terminal(view) => view.read(cx).focus.clone(),
            SpaceContent::Empty => leaf.focus.clone(),
        })
    }
}
