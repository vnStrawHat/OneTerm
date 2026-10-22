//! `PopupMenu` keeps its item list private, so what is testable here are the
//! guards and labels the builder feeds it; the item order itself is reviewed
//! against `low-level-design/input.md` §"Context menu".

use crate::panel::DuplicateDestination;
use crate::space::{SpaceId, SplitDir};

use super::menu::{can_close_space, duplicate_label};

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
