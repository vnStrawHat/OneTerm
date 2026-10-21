//! The "Input Channel" submenu of a tab's right-click menu.
//!
//! It acts on the right-clicked tab: the single-Space rows on the tab's **active** Space (the one
//! its directory, status icon and recording dot report, and the one the key-bound actions act
//! on), the two tab-wide rows on every Space of the tab. Membership itself stays per Space
//! (`DEC-0009`).

use gpui::{App, Context, Entity, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use oneterm_actions::{CloseInputChannel, JoinInputChannel, LeaveInputChannel};
use oneterm_core::InputChannel;

use super::TerminalPanel;

/// One entry of the "Input Channel" submenu, in menu order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ChannelMenuItem {
    /// `Channel A`..`Channel E`; the flag marks the active Space's own channel.
    Join(InputChannel, bool),
    Separator,
    Leave,
    Close(InputChannel),
    JoinTab(InputChannel),
    LeaveTab,
}

/// The submenu entries for a Space in `channel`, inside a tab of `tab_spaces` terminal Spaces of
/// which `tab_has_member` says whether any is a member.
///
/// `PopupMenu` keeps its items private, so the conditions live here as data and
/// [`channel_submenu`] only turns them into items.
pub(super) fn channel_menu_items(
    channel: Option<InputChannel>,
    tab_spaces: usize,
    tab_has_member: bool,
) -> Vec<ChannelMenuItem> {
    let mut items: Vec<_> = InputChannel::ALL
        .into_iter()
        .map(|ch| ChannelMenuItem::Join(ch, Some(ch) == channel))
        .collect();
    if let Some(current) = channel {
        items.push(ChannelMenuItem::Separator);
        items.push(ChannelMenuItem::Leave);
        items.push(ChannelMenuItem::Close(current));
    }
    if tab_spaces > 1 {
        let mut tab_items = Vec::new();
        if let Some(current) = channel {
            tab_items.push(ChannelMenuItem::JoinTab(current));
        }
        if tab_has_member {
            tab_items.push(ChannelMenuItem::LeaveTab);
        }
        if !tab_items.is_empty() {
            items.push(ChannelMenuItem::Separator);
            items.append(&mut tab_items);
        }
    }
    items
}

/// Label of one submenu entry. The popup menu has no radio mark, so the
/// current channel is marked with a `* ` prefix and the others are indented by
/// the same width to keep the letters in one column.
pub(super) fn channel_item_label(item: ChannelMenuItem) -> String {
    match item {
        ChannelMenuItem::Join(channel, true) => format!("* Channel {}", channel.label()),
        ChannelMenuItem::Join(channel, false) => format!("  Channel {}", channel.label()),
        ChannelMenuItem::Separator => String::new(),
        ChannelMenuItem::Leave => "Leave Channel".to_string(),
        ChannelMenuItem::Close(channel) => format!("Close Channel {}", channel.label()),
        ChannelMenuItem::JoinTab(channel) => {
            format!("Join All Spaces In Tab To {}", channel.label())
        }
        ChannelMenuItem::LeaveTab => "Leave With All Spaces In Tab".to_string(),
    }
}

impl TerminalPanel {
    /// The channel of the active Space, if it is a member.
    fn active_channel(&self, cx: &App) -> Option<InputChannel> {
        let view = self.active_view()?;
        self.deps
            .input_channels
            .as_ref()?
            .read(cx)
            .channel_of(view.entity_id())
    }

    /// The submenu's entries for this tab; `None` while the active Space holds no terminal
    /// (an empty Space cannot join a channel).
    pub(super) fn channel_entries(&self, cx: &App) -> Option<Vec<ChannelMenuItem>> {
        self.active_view()?;
        Some(channel_menu_items(
            self.active_channel(cx),
            self.terminal_space_count(),
            !self.tab_channels(cx).is_empty(),
        ))
    }

    /// Apply one submenu entry: the single-Space rows to the active Space, the tab-wide rows to
    /// every Space of the tab.
    pub(super) fn apply_channel_item(&mut self, item: ChannelMenuItem, cx: &mut Context<Self>) {
        match item {
            ChannelMenuItem::Join(channel, _) => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.join_channel(channel, cx));
                }
            }
            ChannelMenuItem::Leave => {
                if let Some(view) = self.active_view() {
                    view.update(cx, |view, cx| view.leave_channel(cx));
                }
            }
            ChannelMenuItem::Close(channel) => {
                if let Some(registry) = self.deps.input_channels.clone() {
                    registry.update(cx, |registry, cx| {
                        registry.close(channel, cx);
                    });
                }
            }
            ChannelMenuItem::JoinTab(channel) => self.join_tab_to_channel(channel, cx),
            ChannelMenuItem::LeaveTab => self.leave_tab_channels(cx),
            ChannelMenuItem::Separator => {}
        }
    }
}

/// Add the "Input Channel" submenu for `panel` to a tab's menu, after a separator. Nothing is
/// added for a tab whose active Space is empty.
pub(super) fn channel_submenu(
    menu: PopupMenu,
    panel: Entity<TerminalPanel>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let Some(items) = panel.read(cx).channel_entries(cx) else {
        return menu;
    };
    menu.separator()
        .submenu("Input Channel", window, cx, move |submenu, _, _| {
            items.iter().fold(submenu, |submenu, item| match item {
                ChannelMenuItem::Separator => submenu.separator(),
                item => submenu.item(channel_item(*item, &panel)),
            })
        })
}

/// One submenu item: the action (for the shortcut hint) plus a click handler that applies the
/// entry to the right-clicked tab, which is not necessarily the active one.
fn channel_item(item: ChannelMenuItem, panel: &Entity<TerminalPanel>) -> PopupMenuItem {
    let panel = panel.clone();
    let menu_item = PopupMenuItem::new(channel_item_label(item));
    let menu_item = match item {
        ChannelMenuItem::Join(channel, _) => menu_item.action(Box::new(JoinInputChannel(channel))),
        ChannelMenuItem::Leave => menu_item.action(Box::new(LeaveInputChannel)),
        ChannelMenuItem::Close(_) => menu_item.action(Box::new(CloseInputChannel)),
        // The tab-wide items are menu-only: nothing binds them to a key.
        _ => menu_item,
    };
    menu_item.on_click(move |_, _, cx| {
        panel.update(cx, |panel, cx| panel.apply_channel_item(item, cx));
    })
}

#[cfg(test)]
mod tests {
    use oneterm_core::InputChannel;

    use super::{ChannelMenuItem, channel_item_label, channel_menu_items};

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

        // Join-all copies the active Space's channel, so it needs one.
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
}
