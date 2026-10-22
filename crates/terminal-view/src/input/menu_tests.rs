//! `PopupMenu` keeps its item list private, so what is testable here are the
//! guards and labels the builder feeds it; the item order itself is reviewed
//! against `low-level-design/input.md` §"Context menu".

use gpui::WeakEntity;
use gpui_component::menu::PopupMenuItem;

use crate::panel::DuplicateDestination;
use crate::space::{SpaceId, SplitDir};

use super::menu::{can_close_space, dead_duplicate_item, duplicate_label, split_item};

#[test]
fn close_space_only_with_siblings() {
    assert!(!can_close_space(0));
    assert!(
        !can_close_space(1),
        "closing the only Space is what Close Terminal Tab does"
    );
    assert!(can_close_space(2));
}

#[test]
fn duplicate_destination_labels() {
    assert_eq!(
        duplicate_label(DuplicateDestination::NewTab),
        "In New Tab".to_string()
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::ExistingSpace(SpaceId(3))),
        format!("Into Space #{}", SpaceId(3).display_number())
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::Split(SplitDir::Right)),
        "Split Right".to_string()
    );
    assert_eq!(
        duplicate_label(DuplicateDestination::Split(SplitDir::Down)),
        "Split Down".to_string()
    );
}

/// `(label, disabled)` of a plain item. The `Item` variant's fields are public and the menu's
/// own list is not, so the rows are checked through the items the builder helpers return.
fn item_state(item: &PopupMenuItem) -> (String, bool) {
    match item {
        PopupMenuItem::Item {
            label, disabled, ..
        } => (label.to_string(), *disabled),
        _ => panic!("expected a plain item"),
    }
}

#[test]
fn split_items_follow_the_session_liveness() {
    let panel = WeakEntity::new_invalid();
    for dir in [
        SplitDir::Right,
        SplitDir::Left,
        SplitDir::Up,
        SplitDir::Down,
    ] {
        let live = item_state(&split_item(dir, &panel, SpaceId(0), None, false));
        let dead = item_state(&split_item(dir, &panel, SpaceId(0), None, true));
        assert!(!live.1, "{} is enabled while the session runs", live.0);
        assert!(dead.1, "{} is disabled once the session ended", dead.0);
        assert_eq!(live.0, dead.0, "liveness changes the state, not the label");
    }
}

#[test]
fn the_dead_duplicate_row_is_a_plain_disabled_row() {
    assert_eq!(
        item_state(&dead_duplicate_item()),
        ("Duplicate Session".to_string(), true),
        "a disabled row, not a submenu"
    );
}
