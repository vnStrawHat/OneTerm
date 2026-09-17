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
/// A terminal page: title, sidebar icon, the description shown under the page
/// header, and the groups it carries.
///
/// A page whose single group would repeat the page's own name carries the
/// description instead and leaves that group untitled, so the name is printed
/// twice (sidebar row, page header) and not three times (`F-R4`).
type TerminalPage = (
    &'static str,
    IconName,
    Option<&'static str>,
    &'static [fn() -> SettingGroup],
);

const TERMINAL_PAGES: &[TerminalPage] = &[
    (
        "Terminal",
        IconName::SquareTerminal,
        None,
        &[font::group, cursor::group],
    ),
    (
        "Terminal Display",
        IconName::LayoutDashboard,
        None,
        &[layout::group, scroll::group, bell::group],
    ),
    (
        "Mouse & Clipboard",
        IconName::Copy,
        None,
        &[mouse::group, security::group],
    ),
    (
        "Terminal Logging",
        IconName::FileText,
        Some("Write printable terminal output to timestamped log files."),
        &[logging::group],
    ),
    (
        "Completion",
        IconName::Search,
        Some("The suggestion overlay and the history it draws on."),
        &[completion::group],
    ),
];

/// The terminal pages, in sidebar order.
pub(crate) fn pages() -> Vec<SettingPage> {
    TERMINAL_PAGES
        .iter()
        .map(|(title, icon, description, groups)| {
            let mut page = SettingPage::new(*title)
                .resettable(true)
                .icon(Icon::new(icon.clone()));
            if let Some(text) = description {
                page = page.description(*text);
            }
            groups.iter().fold(page, |page, group| page.group(group()))
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

    /// Every terminal group builder, from the table.
    fn placed_builders() -> Vec<usize> {
        TERMINAL_PAGES
            .iter()
            .flat_map(|(_, _, _, groups)| groups.iter())
            .map(|group| *group as usize)
            .collect()
    }

    #[test]
    fn no_terminal_page_carries_more_groups_than_have_been_walked() {
        // Nine groups on one page is what `F14` measured as unnavigable. Three
        // is the most that has been walked and shown to keep every sub-item
        // landing at 1016x708 (`US-0122` Evidence), so three is the cap.
        assert_eq!(TERMINAL_PAGES.len(), 5);
        for (title, _, _, groups) in TERMINAL_PAGES {
            assert!(
                !groups.is_empty(),
                "{title} carries no groups and would render an empty page"
            );
            assert!(
                groups.len() <= 3,
                "{title} carries {} groups; add a page rather than a fourth group",
                groups.len()
            );
        }
    }

    #[test]
    fn no_terminal_group_is_placed_on_two_pages() {
        // What this file *can* check. There is no enumerable source of terminal
        // groups to assert coverage against -- each is a function in its own
        // module -- so "every group is on a page" is guarded by the compiler
        // rather than here: a builder left off this table has no caller and
        // `cargo clippy -- -D warnings` fails it as dead code. That is the real
        // guard, and `US-0122` says so rather than claiming a source check.
        let placed = placed_builders();
        let mut unique = placed.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(placed.len(), unique.len(), "a group is on two pages");
        assert_eq!(placed.len(), 9, "the walked page composition changed");
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
    fn a_single_group_page_describes_itself_instead_of_repeating_its_name() {
        // `F-R4`: sidebar row, page header and group heading all said
        // "Completion". A page whose one group would repeat its name carries
        // the description and leaves the group untitled.
        for (title, _, description, groups) in TERMINAL_PAGES {
            if groups.len() == 1 {
                assert!(
                    description.is_some(),
                    "{title} is one group with no page description, so the group must \
                     carry a heading and the name is printed twice over"
                );
            }
        }
    }

    #[test]
    fn the_shell_group_is_hosted_by_general_and_not_by_a_terminal_page() {
        // `US-0122` moved it; this is the assertion that it did not come back
        // and end up on two pages at once.
        let shell = shell::group as fn() -> SettingGroup as usize;
        assert!(!placed_builders().contains(&shell));
    }
}
