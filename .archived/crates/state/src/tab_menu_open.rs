//! Which Terminal Tab's context menu is open (`IN-0055`, `US-0196` rework 8).
//!
//! The kit keeps a context menu's open state private, and the menu is built by the terminal
//! panel's tab row, not by the vertical rail that draws the row's frame. The rail wants to keep
//! the right-clicked row highlighted while its menu is up (the menu's overlay takes the pointer,
//! so the row stops being hovered), so the menu builder says which panel's menu is open here and
//! the rail reads it, the same way [`TabRowLayout`](crate::TabRowLayout) carries the row's
//! layout from the rail to the row.

use gpui::{App, Global};

/// The panel (its entity id, which is also its `PanelId`) whose tab menu is open, if any, and the
/// menu itself (its entity id), so that the end of one menu never clears the record of the next
/// one opened on the same tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabMenuOpen(Option<(u64, u64)>);

impl Global for TabMenuOpen {}

impl TabMenuOpen {
    /// The panel whose tab menu is open now.
    pub fn panel(cx: &App) -> Option<u64> {
        cx.try_global::<Self>()
            .and_then(|open| open.0)
            .map(|(panel, _)| panel)
    }

    /// `panel`'s tab menu, the entity `menu`, opened. A menu opened later (on this tab or
    /// another) replaces the record: only one context menu is open at a time.
    pub fn opened(panel: u64, menu: u64, cx: &mut App) {
        cx.set_global(Self(Some((panel, menu))));
    }

    /// The menu `menu` closed (dismissed or released). A late close of a menu that a newer one
    /// has since replaced leaves the newer record alone.
    pub fn closed(menu: u64, cx: &mut App) {
        if cx
            .try_global::<Self>()
            .and_then(|open| open.0)
            .map(|(_, open)| open)
            == Some(menu)
        {
            cx.set_global(Self(None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn the_newest_menu_wins_and_a_stale_close_is_ignored(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(TabMenuOpen::panel(cx), None);
            TabMenuOpen::opened(7, 100, cx);
            assert_eq!(TabMenuOpen::panel(cx), Some(7));
            // A right-click on another tab opens its menu before the first one's dismissal runs.
            TabMenuOpen::opened(9, 101, cx);
            TabMenuOpen::closed(100, cx);
            assert_eq!(TabMenuOpen::panel(cx), Some(9));
            // The same tab's second menu replaces its first; the first one's end is stale.
            TabMenuOpen::opened(9, 102, cx);
            TabMenuOpen::closed(101, cx);
            assert_eq!(TabMenuOpen::panel(cx), Some(9));
            TabMenuOpen::closed(102, cx);
            assert_eq!(TabMenuOpen::panel(cx), None);
        });
    }
}
