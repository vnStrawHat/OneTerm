//! Terminal settings page assembly and the shared set-and-persist helper.
//!
//! Each terminal settings group lives in its own sibling module so each file owns
//! one cohesive group and stays easy to review.
//!
//! Every terminal group is on one **Terminal** page except Completion (`US-0195`).
//! `US-0122` had spread them over four pages because the kit scrolled a sidebar
//! sub-item by measured height and under-shot below the fold; the 0.7.3 kit
//! scrolls by index, so a long page navigates exactly (`docs/gui-layout.md`
//! §"Sidebar navigation").

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
        &[
            font::group,
            cursor::group,
            layout::group,
            scroll::group,
            bell::group,
            mouse::group,
            security::group,
            logging::group,
        ],
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
    fn no_terminal_page_is_empty() {
        assert_eq!(TERMINAL_PAGES.len(), 2);
        for (title, _, _, groups) in TERMINAL_PAGES {
            assert!(
                !groups.is_empty(),
                "{title} carries no groups and would render an empty page"
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
