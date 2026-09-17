//! Terminal settings page assembly and the shared set-and-persist helper.
//!
//! Each terminal settings group lives in its own sibling module so each file owns
//! one cohesive group and stays easy to review.
//!
//! The groups are spread over several pages rather than stacked on one, because
//! the kit can only scroll a sidebar sub-item into view when the target group has
//! already been laid out — see `docs/gui-layout.md` §"Sidebar navigation" and
//! `US-0122`. A page that fits the window is measured in full, so every one of
//! its sub-items lands exactly; a nine-group page is not, and its later sub-items
//! do not. Keeping each page short is therefore a navigation requirement, not a
//! matter of taste. Adding a group to a page that is already full moves the fold
//! and breaks navigation for everything below it: add a page instead.

mod bell;
mod completion;
mod cursor;
mod font;
mod layout;
mod logging;
mod mouse;
mod scroll;
mod security;
mod shell;

use gpui::App;
use gpui_component::{
    Icon, IconName,
    setting::{SettingGroup, SettingPage},
};
use oneterm_settings::TerminalSettings;

/// The "Shell" group, hosted by the General page (`US-0122`): the shell new
/// local terminals start is one of the first settings a new user looks for, so
/// it sits on the page the Settings window opens on rather than down the
/// Terminal page.
pub(super) fn shell_group() -> SettingGroup {
    shell::group()
}

/// The terminal pages, in sidebar order: title, sidebar icon, and the groups
/// each one carries.
///
/// A page with more than one group gets sidebar sub-items; a page with one gets
/// none and is reached by its own row, which needs no scroll and is always
/// exact. Both are fine — what is not fine is a page so long that a sub-item
/// cannot be scrolled to, which is what this table exists to prevent.
const TERMINAL_PAGES: &[(&str, IconName, &[fn() -> SettingGroup])] = &[
    (
        "Terminal",
        IconName::SquareTerminal,
        &[font::group, cursor::group],
    ),
    (
        "Terminal Display",
        IconName::LayoutDashboard,
        &[layout::group, scroll::group, bell::group],
    ),
    (
        "Mouse & Clipboard",
        IconName::Copy,
        &[mouse::group, security::group],
    ),
    ("Terminal Logging", IconName::FileText, &[logging::group]),
    ("Completion", IconName::Search, &[completion::group]),
];

/// The terminal pages, in sidebar order.
pub(crate) fn pages() -> Vec<SettingPage> {
    TERMINAL_PAGES
        .iter()
        .map(|(title, icon, groups)| {
            groups.iter().fold(
                SettingPage::new(*title)
                    .resettable(true)
                    .icon(Icon::new(icon.clone())),
                |page, group| page.group(group()),
            )
        })
        .collect()
}

/// Apply `f` to the live [`TerminalSettings`], notify, and persist to `terminal.json`.
pub(super) fn set(cx: &mut App, f: impl FnOnce(&mut TerminalSettings)) {
    TerminalSettings::global(cx).update(cx, |s, cx| {
        f(s);
        cx.notify();
    });
    TerminalSettings::persist_global(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_terminal_groups_are_spread_over_pages_none_of_which_is_long() {
        // Nine groups on one page is what `F14` measured as unnavigable. The
        // count is the thing that must not creep back: three groups is the most
        // that has been walked and shown to keep every sub-item landing at
        // 1016x708 (`US-0122` Evidence).
        assert_eq!(TERMINAL_PAGES.len(), 5);
        let total: usize = TERMINAL_PAGES
            .iter()
            .map(|(_, _, groups)| groups.len())
            .sum();
        assert_eq!(total, 9, "a terminal group was added or dropped");
        for (title, _, groups) in TERMINAL_PAGES {
            assert!(
                (1..=3).contains(&groups.len()),
                "{title} carries {} groups; add a page rather than a fourth group",
                groups.len()
            );
        }
    }

    #[test]
    fn every_terminal_page_has_its_own_name() {
        let mut titles: Vec<&str> = TERMINAL_PAGES.iter().map(|(title, ..)| *title).collect();
        let before = titles.len();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(before, titles.len(), "two terminal pages share a name");
    }

    #[test]
    fn the_shell_group_is_hosted_by_general_and_not_by_a_terminal_page() {
        // `US-0122` moved it; this is the assertion that it did not come back
        // and end up on two pages at once.
        let builders: Vec<usize> = TERMINAL_PAGES
            .iter()
            .flat_map(|(_, _, groups)| groups.iter())
            .map(|group| *group as usize)
            .collect();
        let shell = shell::group as fn() -> SettingGroup as usize;
        assert!(!builders.contains(&shell));
    }
}
