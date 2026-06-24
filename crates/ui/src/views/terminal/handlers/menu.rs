//! Context menu cho `LocalTerminalView`.

use gpui::{Entity, FocusHandle, Window};
use gpui_component::menu::{ContextMenu, ContextMenuExt as _, PopupMenuItem};

use myterm2_core::TerminalSession;

use crate::actions::AddPanel;

/// Gắn right-click context menu.
///
/// Layout:
/// 1. New Terminal
/// 2. ── separator ──
/// 3. Copy
/// 4. Paste
/// 5. Select All
/// 6. Clear
/// 7. ── separator ──
/// 8. Close Terminal Tab
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

            // 1. New Terminal — thêm TerminalPanel mới vào center dock.
            menu.item(PopupMenuItem::new("New Terminal").on_click({
                let f = focus.clone();
                move |_, window, cx| {
                    window.dispatch_action(
                        Box::new(AddPanel(gpui_component::dock::DockPlacement::Center)),
                        cx,
                    );
                    window.focus(&f, cx);
                }
            }))
            // 2. ── separator ──
            .separator()
            // 3. Copy
            .item(
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
            // 4. Paste
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
            // 5. Select All
            .item(PopupMenuItem::new("Select All").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.select_all());
                    window.focus(&f, cx);
                }
            }))
            // 6. Clear
            .item(PopupMenuItem::new("Clear").on_click({
                let s = session.clone();
                let f = focus.clone();
                move |_, window, cx| {
                    s.update(cx, |s, _| s.clear());
                    window.focus(&f, cx);
                }
            }))
            // 7. ── separator ──
            .separator()
            // 8. Close Terminal Tab — dispatch ClosePanel action.
            .item(PopupMenuItem::new("Close Terminal Tab").on_click({
                let f = focus.clone();
                move |_, window, cx| {
                    window.dispatch_action(Box::new(gpui_component::dock::ClosePanel), cx);
                    window.focus(&f, cx);
                }
            }))
        }
    })
}
