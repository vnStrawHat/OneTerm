//! Terminal edit commands shared by the keyboard handler, the context menu,
//! and the panel's action handlers: copy / paste / select all / clear.

use gpui::{App, ClipboardItem, Entity};

use oneterm_terminal::TerminalSession;

/// Copy the current selection to the clipboard (no-op when nothing is selected).
pub(crate) fn copy_selection(session: &Entity<Box<dyn TerminalSession>>, cx: &mut App) {
    if let Some(text) = session.read(cx).selection_text() {
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

/// Paste the clipboard text into the terminal (bracketed when the program asks
/// for it — see `TerminalSession::paste`). Snaps the viewport back to the live
/// screen like typed input.
pub(crate) fn paste_clipboard(session: &Entity<Box<dyn TerminalSession>>, cx: &mut App) {
    let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
        return;
    };
    session.update(cx, |s, _| {
        s.scroll_to_bottom();
        s.paste(&text);
    });
}

/// Select the whole scrollback + screen.
pub(crate) fn select_all(session: &Entity<Box<dyn TerminalSession>>, cx: &mut App) {
    session.update(cx, |s, _| s.select_all());
}

/// Clear the screen (and scrollback) like the `clear` command.
pub(crate) fn clear_screen(session: &Entity<Box<dyn TerminalSession>>, cx: &mut App) {
    session.update(cx, |s, _| s.clear());
}
