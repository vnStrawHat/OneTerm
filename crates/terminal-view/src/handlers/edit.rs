//! Terminal edit commands shared by the keyboard handler, the context menu,
//! and the panel's action handlers: copy / paste / select all / clear.

use gpui::{App, ClipboardItem, Entity, Window};
use gpui_component::{WindowExt as _, notification::NotificationType};

use oneterm_state::notif_ext::notify;
use oneterm_terminal::TerminalSession;

/// One terminal edit command: shared signature for the keyboard handler, the
/// context menu and the panel actions. `Window` is available on every path and
/// lets a failed user action surface as a notification (ERR-04).
pub(crate) type EditCommand = fn(&Entity<Box<dyn TerminalSession>>, &mut Window, &mut App);

/// Copy the current selection to the clipboard (no-op when nothing is selected).
pub(crate) fn copy_selection(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    if let Some(text) = session.read(cx).selection_text() {
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

/// Paste the clipboard text into the terminal (bracketed when the program asks
/// for it — see `TerminalSession::paste`). Snaps the viewport back to the live
/// screen like typed input. A rejected or undeliverable paste is reported to
/// the user instead of being dropped silently (ERR-04).
pub(crate) fn paste_clipboard(
    session: &Entity<Box<dyn TerminalSession>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return;
    };
    let result = session.update(cx, |s, _| {
        s.scroll_to_bottom();
        s.paste(&text)
    });
    if let Err(error) = result {
        log::warn!("terminal paste failed: {error}");
        window.push_notification(notify(NotificationType::Warning, error.to_string(), cx), cx);
    }
}

/// Select the whole scrollback + screen.
pub(crate) fn select_all(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    session.update(cx, |s, _| s.select_all());
}

/// Clear the screen (and scrollback) like the `clear` command.
pub(crate) fn clear_screen(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    session.update(cx, |s, _| s.clear());
}
