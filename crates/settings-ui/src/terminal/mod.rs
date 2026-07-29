//! Terminal settings page assembly and shared persistence helper.
//!
//! Each terminal settings group lives in its own sibling module so each file owns
//! one cohesive group and stays easy to review.

mod bell;
mod cursor;
mod font;
mod layout;
mod mouse;
mod scroll;
mod security;
mod shell;

use gpui::App;
use gpui_component::{Icon, IconName, setting::SettingPage};
use oneterm_settings::TerminalSettings;

/// Build the "Terminal" settings page.
pub(crate) fn page() -> SettingPage {
    SettingPage::new("Terminal")
        .resettable(true)
        .icon(Icon::new(IconName::SquareTerminal))
        .group(shell::group())
        .group(font::group())
        .group(cursor::group())
        .group(layout::group())
        .group(scroll::group())
        .group(mouse::group())
        .group(bell::group())
        .group(security::group())
}

/// Persist the live [`TerminalSettings`] to `terminal.json`.
pub(super) fn persist(cx: &mut App) {
    TerminalSettings::persist_global(cx);
}
