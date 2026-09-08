//! The four terminal edit commands, shared by the keyboard, the context menu
//! and the panel's action handlers: copy / paste / select all / clear.

use gpui::{App, ClipboardItem, Entity, Window};
use gpui_component::{WindowExt as _, notification::NotificationType};

use oneterm_terminal::TerminalSession;
use oneterm_terminal::session::PasteError;
use oneterm_theme::notif_ext::notify;

/// One terminal edit command. `Window` is available on every call path and
/// lets a failed user action surface as a notification (ERR-04).
pub(crate) type EditCommand = fn(&Entity<Box<dyn TerminalSession>>, &mut Window, &mut App);

/// Copy the current selection. Copying with nothing selected is a no-op, not
/// an empty clipboard.
pub(crate) fn copy_selection(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    if let Some(text) = session.read(cx).selection_text()
        && !text.is_empty()
    {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// Paste the clipboard into the terminal. Bracketed or plain is the backend's
/// decision; a rejected paste is reported instead of silently dropped (ERR-04).
pub(crate) fn paste_clipboard(
    session: &Entity<Box<dyn TerminalSession>>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return;
    };
    if let Err(error) = paste_text(session, &text, cx) {
        log::warn!("terminal paste failed: {error}");
        window.push_notification(notify(NotificationType::Warning, error.to_string(), cx), cx);
    }
}

/// The session half of a paste: typing-like input snaps the viewport back to
/// the live screen first. Separated from the toast so it is testable without a
/// window.
fn paste_text(
    session: &Entity<Box<dyn TerminalSession>>,
    text: &str,
    cx: &mut App,
) -> Result<(), PasteError> {
    session.update(cx, |s, _| {
        s.scroll_to_bottom();
        s.paste(text)
    })
}

/// Select the whole scrollback and screen.
pub(crate) fn select_all(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    session.update(cx, |s, _| s.select_all());
}

/// Clear the screen and scrollback, like the `clear` command.
pub(crate) fn clear_screen(
    session: &Entity<Box<dyn TerminalSession>>,
    _window: &mut Window,
    cx: &mut App,
) {
    session.update(cx, |s, _| s.clear());
}
