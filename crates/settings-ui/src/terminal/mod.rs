//! Terminal settings page assembly and the shared set-and-persist helper.
//!
//! Each terminal settings group lives in its own sibling module so each file owns
//! one cohesive group and stays easy to review.

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
        .group(logging::group())
        .group(scroll::group())
        .group(mouse::group())
        .group(bell::group())
        .group(security::group())
        .group(completion::group())
}

/// Apply `f` to the live [`TerminalSettings`], notify, and persist to `terminal.json`.
pub(super) fn set(cx: &mut App, f: impl FnOnce(&mut TerminalSettings)) {
    TerminalSettings::global(cx).update(cx, |s, cx| {
        f(s);
        cx.notify();
    });
    TerminalSettings::persist_global(cx);
}
