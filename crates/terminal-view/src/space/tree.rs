//! The Space pane tree that lives inside a `TerminalPanel`.
//!
//! A [`SpaceTree`] is a tree of [`SpaceNode`]s: leaves are Spaces (a terminal
//! view or an empty placeholder), internal nodes are N-ary splits along one
//! [`Axis`] whose sizing is fully delegated to one
//! [`ResizableState`](gpui_component::resizable::ResizableState) per level.
//!
//! Every mutation is a small in-place transform (`split`, `close`,
//! `fill_empty`, `take_leaf_terminal`); the tree owns no GPUI context, so leaf
//! focus handles and split states are created by the panel and passed in.
//! Rendering lives in [`super::render`].

use gpui::{Axis, Entity, FocusHandle, WeakEntity};
use gpui_component::resizable::ResizableState;

use crate::panel::TerminalPanel;
use crate::terminal_view::TerminalView;

/// Stable identity for a Space leaf — used for active tracking, focus routing,
/// and drop targeting. Stable for the lifetime of the leaf.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub(crate) struct SpaceId(pub u64);

impl SpaceId {
    /// User-facing Space number. Production empty Spaces always have ids >= 1.
    pub(crate) fn display_number(self) -> u64 {
        self.0
    }
}

/// Direction to split a Space in.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum SplitDir {
    Right,
    Left,
    Up,
    Down,
}

impl SplitDir {
    /// The layout axis for this direction.
    pub(crate) fn axis(self) -> Axis {
        match self {
            SplitDir::Right | SplitDir::Left => Axis::Horizontal,
            SplitDir::Up | SplitDir::Down => Axis::Vertical,
        }
    }

    /// Whether the new (empty) child is inserted after the existing one.
    pub(crate) fn new_after(self) -> bool {
        matches!(self, SplitDir::Right | SplitDir::Down)
    }
}

/// Result of closing a Space.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum CloseOutcome {
    /// A Space was removed; the tab still has >= 1 Space.
    Removed,
    /// The closed Space was the last one → the whole tab should close.
    LastSpaceClosed,
}

/// Context threaded into a terminal's context menu so its Split / Close-Space
/// items can target the right Space in the right panel. Cloned onto each
/// terminal view via [`TerminalView`]'s `split_ctx` field.
#[derive(Clone)]
pub(crate) struct SplitContext {
    /// The panel owning the Space tree this terminal lives in.
    pub panel: WeakEntity<TerminalPanel>,
    /// The Space (leaf) this terminal occupies.
    pub space_id: SpaceId,
}

/// One node of the Space tree.
pub(crate) enum SpaceNode {
    /// A leaf Space — holds a terminal or is empty.
    Leaf(SpaceLeaf),
    /// A split of two-or-more children along one axis.
    Split(SpaceSplit),
}

/// A leaf Space.
///
/// `Clone` duplicates reference-counted handles only (the entity and the focus
/// handle both point at the same objects); [`split`](SpaceTree::split) uses it
/// to re-seat a leaf under a freshly created split node.
#[derive(Clone)]
pub(crate) struct SpaceLeaf {
    pub id: SpaceId,
    pub content: SpaceContent,
    /// Focus target for the empty placeholder (the terminal leaf uses the
    /// view's own focus handle instead).
    pub focus: FocusHandle,
}

/// The content of a leaf Space.
#[derive(Clone)]
pub(crate) enum SpaceContent {
    /// A live terminal view (local or SSH — both are `TerminalView`).
    Terminal(Entity<TerminalView>),
    /// Empty Space: renders a placeholder.
    Empty,
}

/// An internal split node: `children` laid out along `axis`, sharing `state`.
pub(crate) struct SpaceSplit {
    /// Horizontal split = children laid out left→right (Split Right/Left).
    /// Vertical split = children laid out top→bottom (Split Up/Down).
    pub axis: Axis,
    /// Invariant: a split always has at least two children — `close` collapses
    /// a split into its last remaining child.
    pub children: Vec<SpaceNode>,
    /// Sizes/handles for this split level (one entity per split node).
    pub state: Entity<ResizableState>,
}

/// The pane tree inside a `TerminalPanel`.
pub(crate) struct SpaceTree {
    root: SpaceNode,
    /// `SpaceId` allocator.
    next_id: u64,
    /// The active (focused) leaf.
    active: SpaceId,
}

impl SpaceTree {
    /// Create a tree with a single empty leaf (no terminal session).
    pub(crate) fn new_empty(focus: FocusHandle) -> Self {
        // SpaceId(0) is reserved for a tab's initial terminal. This recovery
        // constructor is used only when spawning that terminal fails, so its
        // visible empty placeholder starts at Space #1 like split-created leaves.
        Self::single(SpaceId(1), SpaceContent::Empty, focus, 2)
    }

    /// Create a tree with a single terminal leaf wrapping `view`.
    pub(crate) fn new_terminal(view: Entity<TerminalView>, focus: FocusHandle) -> Self {
        Self::single(SpaceId(0), SpaceContent::Terminal(view), focus, 1)
    }

    fn single(id: SpaceId, content: SpaceContent, focus: FocusHandle, next_id: u64) -> Self {
        Self {
            root: SpaceNode::Leaf(SpaceLeaf { id, content, focus }),
            next_id,
            active: id,
        }
    }

    /// Allocate a fresh, unused `SpaceId`.
    pub(crate) fn alloc_id(&mut self) -> SpaceId {
        let id = SpaceId(self.next_id);
        self.next_id += 1;
        id
    }

    /// The root node (for rendering and structure tests).
    pub(crate) fn root(&self) -> &SpaceNode {
        &self.root
    }

    /// Whether the tree is a single leaf (no splits) — the "plain terminal"
    /// fast path with no Space chrome.
    pub(crate) fn is_single(&self) -> bool {
        matches!(self.root, SpaceNode::Leaf(_))
    }

    /// The number of leaf Spaces.
    pub(crate) fn leaf_count(&self) -> usize {
        self.root.leaf_count()
    }

    /// The id of the first leaf in tree order.
    pub(crate) fn first_leaf_id(&self) -> SpaceId {
        self.root.first_leaf_id()
    }

    /// The 0-based depth-first index of leaf `id` (for the Agent Panel `#N`
    /// Space label). `None` if `id` is not a leaf.
    pub(crate) fn leaf_index(&self, id: SpaceId) -> Option<usize> {
        let mut index = 0;
        self.root.leaf_index(id, &mut index)
    }

    /// The active leaf id.
    pub(crate) fn active(&self) -> SpaceId {
        self.active
    }

    /// Set the active leaf id (no-op if the id is not a leaf).
    pub(crate) fn set_active(&mut self, id: SpaceId) {
        if self.has_leaf(id) {
            self.active = id;
        }
    }

    /// Whether `id` is currently a leaf in the tree.
    pub(crate) fn has_leaf(&self, id: SpaceId) -> bool {
        self.root.find_leaf(id).is_some()
    }

    /// The active leaf's terminal view, if the active Space holds one.
    pub(crate) fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.leaf_terminal(self.active)
    }

    /// The terminal view held by leaf `id`, if any.
    pub(crate) fn leaf_terminal(&self, id: SpaceId) -> Option<Entity<TerminalView>> {
        match &self.root.find_leaf(id)?.content {
            SpaceContent::Terminal(view) => Some(view.clone()),
            SpaceContent::Empty => None,
        }
    }

    /// Empty Space ids in rendered tree order; their display numbers are the
    /// ids themselves, so filling or closing one never renumbers the others.
    pub(crate) fn empty_space_destinations(&self) -> Vec<SpaceId> {
        let mut out = Vec::new();
        self.root.collect_leaves(&mut |leaf| {
            if matches!(leaf.content, SpaceContent::Empty) {
                out.push(leaf.id);
            }
        });
        out
    }

    /// Every terminal view in the tree (used to (re)subscribe to title events).
    pub(crate) fn terminal_views(&self) -> Vec<Entity<TerminalView>> {
        let mut out = Vec::new();
        self.root.collect_leaves(&mut |leaf| {
            if let SpaceContent::Terminal(view) = &leaf.content {
                out.push(view.clone());
            }
        });
        out
    }

    /// Whether the tree contains no terminal leaves (all empty).
    pub(crate) fn has_no_terminals(&self) -> bool {
        self.terminal_views().is_empty()
    }

    /// The focus handle for the active leaf: the terminal view's handle for a
    /// terminal leaf, or the placeholder's handle for an empty leaf.
    pub(crate) fn active_focus_handle(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let leaf = self.root.find_leaf(self.active)?;
        Some(match &leaf.content {
            SpaceContent::Terminal(view) => view.read(cx).focus.clone(),
            SpaceContent::Empty => leaf.focus.clone(),
        })
    }

    /// Split leaf `target` in `dir`, inserting `empty` (a pre-built empty leaf)
    /// as a new sibling under a fresh split node carrying `state`. The new
    /// empty leaf becomes active. No-op if `target` is not a leaf.
    pub(crate) fn split(
        &mut self,
        target: SpaceId,
        dir: SplitDir,
        empty: SpaceLeaf,
        state: Entity<ResizableState>,
    ) {
        let new_id = empty.id;
        if insert_split(&mut self.root, target, dir, (empty, state)).is_ok() {
            self.active = new_id;
        }
    }

    /// Close leaf `target`, collapsing the tree to keep it well-formed. Returns
    /// the removed terminal view (if the leaf held one) so the caller can close
    /// its session, plus the [`CloseOutcome`].
    pub(crate) fn close(
        &mut self,
        target: SpaceId,
    ) -> (CloseOutcome, Option<Entity<TerminalView>>) {
        if self.leaf_count() <= 1 {
            return (CloseOutcome::LastSpaceClosed, None);
        }
        let removed = remove_leaf(&mut self.root, target);

        // Pick a new active leaf if the active one was the one removed.
        if !self.has_leaf(self.active) {
            self.active = self.first_leaf_id();
        }

        let view = match removed {
            Some(SpaceContent::Terminal(view)) => Some(view),
            Some(SpaceContent::Empty) | None => None,
        };
        (CloseOutcome::Removed, view)
    }

    /// Replace the empty content of leaf `target` with a terminal `view`, which
    /// becomes the active Space. On `Err` the target was missing or occupied
    /// and the view is handed back untouched (never leaked).
    pub(crate) fn fill_empty(
        &mut self,
        target: SpaceId,
        view: Entity<TerminalView>,
    ) -> Result<(), Entity<TerminalView>> {
        if let Some(leaf) = self.root.find_leaf_mut(target)
            && matches!(leaf.content, SpaceContent::Empty)
        {
            leaf.content = SpaceContent::Terminal(view);
            self.active = target;
            return Ok(());
        }
        Err(view)
    }

    /// Take the terminal view out of leaf `id`, leaving the leaf empty. Returns
    /// the removed view, or `None` if the leaf was already empty / missing.
    pub(crate) fn take_leaf_terminal(&mut self, id: SpaceId) -> Option<Entity<TerminalView>> {
        let leaf = self.root.find_leaf_mut(id)?;
        match std::mem::replace(&mut leaf.content, SpaceContent::Empty) {
            SpaceContent::Terminal(view) => Some(view),
            SpaceContent::Empty => None,
        }
    }
}

impl SpaceNode {
    /// Find the leaf with `id` in this subtree.
    fn find_leaf(&self, id: SpaceId) -> Option<&SpaceLeaf> {
        match self {
            SpaceNode::Leaf(leaf) => (leaf.id == id).then_some(leaf),
            SpaceNode::Split(split) => split.children.iter().find_map(|c| c.find_leaf(id)),
        }
    }

    /// Find the leaf with `id` in this subtree (mutable).
    fn find_leaf_mut(&mut self, id: SpaceId) -> Option<&mut SpaceLeaf> {
        match self {
            SpaceNode::Leaf(leaf) => (leaf.id == id).then_some(leaf),
            SpaceNode::Split(split) => split.children.iter_mut().find_map(|c| c.find_leaf_mut(id)),
        }
    }

    /// Total number of leaf Spaces in this subtree.
    fn leaf_count(&self) -> usize {
        match self {
            SpaceNode::Leaf(_) => 1,
            SpaceNode::Split(split) => split.children.iter().map(|c| c.leaf_count()).sum(),
        }
    }

    /// The id of the first leaf in tree order.
    pub(super) fn first_leaf_id(&self) -> SpaceId {
        match self {
            SpaceNode::Leaf(leaf) => leaf.id,
            // A split always has children (see `SpaceSplit::children`); an
            // empty one would have been collapsed away by `close`.
            SpaceNode::Split(split) => match split.children.first() {
                Some(first) => first.first_leaf_id(),
                None => SpaceId(0),
            },
        }
    }

    /// Visit every leaf in this subtree in depth-first (left→right) order.
    fn collect_leaves(&self, visit: &mut impl FnMut(&SpaceLeaf)) {
        match self {
            SpaceNode::Leaf(leaf) => visit(leaf),
            SpaceNode::Split(split) => {
                for child in &split.children {
                    child.collect_leaves(visit);
                }
            }
        }
    }

    /// The 0-based depth-first index of leaf `id`, counting leaves into
    /// `index`. Used for the Agent Panel's stable `#N` Space ordering
    /// (`docs/agent-panel-display.md` §5.1 / §14.1).
    fn leaf_index(&self, id: SpaceId, index: &mut usize) -> Option<usize> {
        match self {
            SpaceNode::Leaf(leaf) if leaf.id == id => Some(*index),
            SpaceNode::Leaf(_) => {
                *index += 1;
                None
            }
            SpaceNode::Split(split) => split
                .children
                .iter()
                .find_map(|child| child.leaf_index(id, index)),
        }
    }
}

/// Wrap leaf `target` in a new split node holding the payload's empty leaf.
/// Returns the payload unused (`Err`) when `target` is not in this subtree.
fn insert_split(
    node: &mut SpaceNode,
    target: SpaceId,
    dir: SplitDir,
    payload: (SpaceLeaf, Entity<ResizableState>),
) -> Result<(), (SpaceLeaf, Entity<ResizableState>)> {
    match node {
        SpaceNode::Leaf(leaf) => {
            if leaf.id != target {
                return Err(payload);
            }
            let (empty, state) = payload;
            let existing = SpaceNode::Leaf(leaf.clone());
            let inserted = SpaceNode::Leaf(empty);
            let children = if dir.new_after() {
                vec![existing, inserted]
            } else {
                vec![inserted, existing]
            };
            *node = SpaceNode::Split(SpaceSplit {
                axis: dir.axis(),
                children,
                state,
            });
            Ok(())
        }
        SpaceNode::Split(split) => {
            let mut payload = payload;
            for child in &mut split.children {
                match insert_split(child, target, dir, payload) {
                    Ok(()) => return Ok(()),
                    Err(unused) => payload = unused,
                }
            }
            Err(payload)
        }
    }
}

/// Remove leaf `target` from this subtree and return its content. The root leaf
/// itself is never removed (callers guard with `leaf_count > 1`), so a leaf is
/// always removed by its parent split, which then collapses if one child is left.
fn remove_leaf(node: &mut SpaceNode, target: SpaceId) -> Option<SpaceContent> {
    let SpaceNode::Split(split) = node else {
        return None;
    };
    let child_leaf = split
        .children
        .iter()
        .position(|child| matches!(child, SpaceNode::Leaf(leaf) if leaf.id == target));
    if let Some(index) = child_leaf {
        let content = match split.children.remove(index) {
            SpaceNode::Leaf(leaf) => Some(leaf.content),
            // Unreachable: `position` above matched a leaf.
            SpaceNode::Split(_) => None,
        };
        collapse_single_child(node);
        return content;
    }

    for child in &mut split.children {
        if let Some(content) = remove_leaf(child, target) {
            return Some(content);
        }
    }
    None
}

/// Replace a split that has exactly one child left with that child.
fn collapse_single_child(node: &mut SpaceNode) {
    let SpaceNode::Split(split) = node else {
        return;
    };
    if split.children.len() != 1 {
        return;
    }
    let only = split.children.remove(0);
    *node = only;
}
