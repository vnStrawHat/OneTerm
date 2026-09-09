//! `PopupMenu` keeps its item list private, so what is testable here are the
//! guards and labels the builder feeds it; the item order itself is reviewed
//! against `low-level-design/input.md` §"Context menu".

use oneterm_core::InputChannel;

use crate::panel::DuplicateDestination;
use crate::space::{SpaceId, SplitDir};

use super::menu::{
    ChannelMenuItem, can_close_space, channel_item_label, channel_menu_items, duplicate_label,
};

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

#[test]
fn a_non_member_space_is_offered_the_five_channels_only() {
    assert_eq!(
        channel_menu_items(None, 1, false),
        InputChannel::ALL
            .into_iter()
            .map(|channel| ChannelMenuItem::Join(channel, false))
            .collect::<Vec<_>>()
    );
}

#[test]
fn leave_and_close_need_a_membership() {
    let items = channel_menu_items(Some(InputChannel::B), 1, true);

    assert!(items.contains(&ChannelMenuItem::Join(InputChannel::B, true)));
    assert!(items.contains(&ChannelMenuItem::Join(InputChannel::A, false)));
    assert_eq!(
        &items[5..],
        &[
            ChannelMenuItem::Separator,
            ChannelMenuItem::Leave,
            ChannelMenuItem::Close(InputChannel::B),
        ]
    );
}

#[test]
fn the_tab_wide_items_need_siblings() {
    // A lone Space never offers them, whatever its membership.
    let alone = channel_menu_items(Some(InputChannel::A), 1, true);
    assert!(!alone.contains(&ChannelMenuItem::JoinTab(InputChannel::A)));
    assert!(!alone.contains(&ChannelMenuItem::LeaveTab));

    // Join-all copies this Space's channel, so it needs one.
    let non_member_in_split = channel_menu_items(None, 3, true);
    assert_eq!(
        &non_member_in_split[5..],
        &[ChannelMenuItem::Separator, ChannelMenuItem::LeaveTab]
    );

    // Nothing to leave in the tab: no section at all.
    assert_eq!(channel_menu_items(None, 3, false).len(), 5);

    let member_in_split = channel_menu_items(Some(InputChannel::A), 3, true);
    assert_eq!(
        &member_in_split[8..],
        &[
            ChannelMenuItem::Separator,
            ChannelMenuItem::JoinTab(InputChannel::A),
            ChannelMenuItem::LeaveTab,
        ]
    );
}

#[test]
fn the_current_channel_is_the_only_marked_item() {
    assert_eq!(
        channel_item_label(ChannelMenuItem::Join(InputChannel::B, true)),
        "* Channel B"
    );
    assert_eq!(
        channel_item_label(ChannelMenuItem::Join(InputChannel::B, false)),
        "  Channel B"
    );
    assert_eq!(
        channel_item_label(ChannelMenuItem::Close(InputChannel::E)),
        "Close Channel E"
    );
    assert_eq!(
        channel_item_label(ChannelMenuItem::JoinTab(InputChannel::A)),
        "Join All Spaces In Tab To A"
    );
    assert_eq!(
        channel_item_label(ChannelMenuItem::LeaveTab),
        "Leave With All Spaces In Tab"
    );
}
