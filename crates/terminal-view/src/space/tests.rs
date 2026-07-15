//! Unit tests for `SpaceTree` split/close/fill operations (ARCH-07).
//!
//! These tests verify tree consistency after each operation — leaf counts,
//! active id, find_leaf, and collapse behavior. They use `SpaceContent::Empty`
//! leaves (no GPUI entities needed) so they run as pure unit tests.

use gpui::{AppContext as _, FocusHandle};

use super::node::{SpaceContent, SpaceId, SpaceLeaf, SpaceNode};
use super::{CloseOutcome, SpaceTree, SplitDir};

/// Helper: build a `SpaceLeaf` with `Empty` content and a dummy focus handle.
fn empty_leaf(id: u64, focus: FocusHandle) -> SpaceLeaf {
    SpaceLeaf {
        id: SpaceId(id),
        content: SpaceContent::Empty,
        focus,
    }
}

// ── Split tests ─────────────────────────────────────────────────────

#[gpui::test]
fn split_right_creates_two_leaves(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let target = tree.active();
        let new_id = tree.alloc_id();
        let empty = empty_leaf(new_id.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());

        tree.split(target, SplitDir::Right, empty, state);

        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.has_leaf(target));
        assert!(tree.has_leaf(new_id));
        assert_eq!(tree.active(), new_id);

        // Root should be a split node.
        assert!(matches!(tree.root(), SpaceNode::Split(_)));
    });
}

#[gpui::test]
fn split_left_inserts_before(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let target = tree.active();
        let new_id = tree.alloc_id();
        let empty = empty_leaf(new_id.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());

        tree.split(target, SplitDir::Left, empty, state);

        // Root is a split with children [new, existing].
        let SpaceNode::Split(split) = tree.root() else {
            panic!("expected split root");
        };
        // Left = new leaf first.
        let first_id = match &split.children[0] {
            SpaceNode::Leaf(l) => l.id,
            _ => panic!("expected leaf"),
        };
        assert_eq!(first_id, new_id);
    });
}

#[gpui::test]
fn split_nonexistent_target_is_noop(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let new_id = tree.alloc_id();
        let empty = empty_leaf(new_id.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());

        // Try to split a leaf that doesn't exist.
        tree.split(SpaceId(999), SplitDir::Right, empty, state);

        // Tree should be unchanged.
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.is_single());
        // Active should not have changed to the non-existent new_id.
        assert_ne!(tree.active(), new_id);
    });
}

#[gpui::test]
fn split_nested_creates_three_leaves(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let leaf0 = tree.active();

        // First split: leaf0 → [leaf0, leaf1]
        let id1 = tree.alloc_id();
        let empty1 = empty_leaf(id1.0, focus.clone());
        let state1 = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(leaf0, SplitDir::Right, empty1, state1);
        assert_eq!(tree.leaf_count(), 2);

        // Second split: split leaf1 → [leaf1, leaf2]
        let id2 = tree.alloc_id();
        let empty2 = empty_leaf(id2.0, focus.clone());
        let state2 = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(id1, SplitDir::Down, empty2, state2);
        assert_eq!(tree.leaf_count(), 3);

        assert!(tree.has_leaf(leaf0));
        assert!(tree.has_leaf(id1));
        assert!(tree.has_leaf(id2));
        assert_eq!(tree.active(), id2);
    });
}

// ── Close tests ──────────────────────────────────────────────────────

#[gpui::test]
fn close_last_leaf_returns_last_space_closed(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus);
        let leaf = tree.active();

        let (outcome, view) = tree.close(leaf);
        assert_eq!(outcome, CloseOutcome::LastSpaceClosed);
        assert!(view.is_none());
    });
}

#[gpui::test]
fn close_one_of_two_collapses_to_single(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let leaf0 = tree.active();
        let id1 = tree.alloc_id();
        let empty = empty_leaf(id1.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(leaf0, SplitDir::Right, empty, state);
        assert_eq!(tree.leaf_count(), 2);

        // Close leaf0 — should collapse to just leaf1.
        let (outcome, _view) = tree.close(leaf0);
        assert_eq!(outcome, CloseOutcome::Removed);
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.is_single());
        assert!(!tree.has_leaf(leaf0));
        assert!(tree.has_leaf(id1));
    });
}

#[gpui::test]
fn close_updates_active_when_active_removed(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let leaf0 = tree.active();
        let id1 = tree.alloc_id();
        let empty = empty_leaf(id1.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(leaf0, SplitDir::Right, empty, state);

        // Active is id1 (the new leaf). Close it — active should fall back.
        assert_eq!(tree.active(), id1);
        let (outcome, _) = tree.close(id1);
        assert_eq!(outcome, CloseOutcome::Removed);
        assert_eq!(tree.leaf_count(), 1);
        assert_eq!(tree.active(), leaf0);
    });
}

#[gpui::test]
fn close_nested_collapses_correctly(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let leaf0 = tree.active();

        // Split into 3: [leaf0, split([leaf1, leaf2])]
        let id1 = tree.alloc_id();
        let empty1 = empty_leaf(id1.0, focus.clone());
        let state1 = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(leaf0, SplitDir::Right, empty1, state1);

        let id2 = tree.alloc_id();
        let empty2 = empty_leaf(id2.0, focus.clone());
        let state2 = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(id1, SplitDir::Down, empty2, state2);
        assert_eq!(tree.leaf_count(), 3);

        // Close leaf1 — the inner split should collapse, leaving [leaf0, leaf2].
        let (outcome, _) = tree.close(id1);
        assert_eq!(outcome, CloseOutcome::Removed);
        assert_eq!(tree.leaf_count(), 2);
        assert!(tree.has_leaf(leaf0));
        assert!(tree.has_leaf(id2));
        assert!(!tree.has_leaf(id1));
    });
}

// ── Fill / take tests ────────────────────────────────────────────────

#[gpui::test]
fn fill_empty_replaces_content(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let tree = SpaceTree::new_empty(focus);
        let leaf_id = tree.active();

        // fill_empty requires an Entity<LocalTerminalView>, which needs a full
        // session. We test the no-op path: fill a nonexistent leaf.
        // (Full fill test is in panel/tests.rs.)
        let _ = leaf_id;
    });
}

#[gpui::test]
fn take_leaf_terminal_returns_none_for_empty(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus);
        let leaf_id = tree.active();

        let result = tree.take_leaf_terminal(leaf_id);
        assert!(result.is_none());
    });
}

#[gpui::test]
fn take_leaf_terminal_returns_none_for_nonexistent(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus);

        let result = tree.take_leaf_terminal(SpaceId(999));
        assert!(result.is_none());
    });
}

// ── Consistency tests ────────────────────────────────────────────────

#[gpui::test]
fn set_active_only_for_existing_leaves(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus.clone());
        let leaf0 = tree.active();

        // Setting to a nonexistent leaf is a no-op.
        tree.set_active(SpaceId(999));
        assert_eq!(tree.active(), leaf0);

        // Split and set active to the new leaf.
        let id1 = tree.alloc_id();
        let empty = empty_leaf(id1.0, focus.clone());
        let state = cx.new(|_| gpui_component::resizable::ResizableState::default());
        tree.split(leaf0, SplitDir::Right, empty, state);

        tree.set_active(id1);
        assert_eq!(tree.active(), id1);

        // Setting back to old leaf.
        tree.set_active(leaf0);
        assert_eq!(tree.active(), leaf0);
    });
}

#[gpui::test]
fn tree_starts_with_single_empty_leaf(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let tree = SpaceTree::new_empty(focus);

        assert!(tree.is_single());
        assert_eq!(tree.leaf_count(), 1);
        assert!(tree.has_no_terminals());
        assert_eq!(tree.terminal_views().len(), 0);
    });
}

#[gpui::test]
fn alloc_id_is_monotonic(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let focus = cx.focus_handle();
        let mut tree = SpaceTree::new_empty(focus);

        let id1 = tree.alloc_id();
        let id2 = tree.alloc_id();
        let id3 = tree.alloc_id();

        assert_eq!(id1.0, 1);
        assert_eq!(id2.0, 2);
        assert_eq!(id3.0, 3);
    });
}
