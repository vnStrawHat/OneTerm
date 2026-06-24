//! Context menu cho `LocalTerminalView`.

use gpui::{Entity, FocusHandle, Window};
use gpui_component::menu::{ContextMenu, ContextMenuExt as _, PopupMenuItem};

use myterm2_core::TerminalSession;

/// Gắn right-click context menu.
pub(crate) fn attach_context_menu<E>(
    div: E,
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
) -> ContextMenu<E>
where
    E: gpui::InteractiveElement + gpui::ParentElement + gpui::Styled,
{
    div.context_menu({
        let session = session.clone();
        let focus = focus.clone();
        move |menu, _window: &mut Window, cx| {
            let has_selection = session
                .read(cx)
                .selection_text()
                .map(|t| !t.is_empty())
                .unwrap_or(false);

            menu.item(
                PopupMenuItem::new("Copy")
                    .disabled(!has_selection)
                    .on_click({
                        let s = session.clone();
                        let f = focus.clone();
                        move |_, window, cx| {
                            if let Some(text) = s.read(cx).selection_text() {
                                if !text.is_empty() {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                                }
                            }
                            window.focus(&f, cx);
                        }
                    }),
            )
            .item(PopupMenuItem::new("Paste").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            s.update(cx, |s, _| s.paste(&text));
                        }
                    }
                    window.focus(&f, cx);
                }
            }))
            .separator()
            .item(PopupMenuItem::new("Select All").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.select_all());
                    window.focus(&f, cx);
                }
            }))
            .item(PopupMenuItem::new("Clear").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.clear());
                    window.focus(&f, cx);
                }
            }))
        }
    })
}
