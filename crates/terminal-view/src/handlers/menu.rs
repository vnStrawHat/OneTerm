//! Context menu for `LocalTerminalView`.

use gpui::{Entity, FocusHandle, Window};
use gpui_component::menu::{ContextMenu, ContextMenuExt as _, PopupMenuItem};

use oneterm_terminal::TerminalSession;

use oneterm_actions::{
    AddPanel, CloseSpace, DuplicateSession, SplitDown, SplitLeft, SplitRight, SplitUp,
    TerminalClear, TerminalCopy, TerminalPaste, TerminalSelectAll,
};

use super::super::panel::DuplicateDestination;
use super::super::space::{SplitContext, SplitDir};

/// Attach the right-click context menu.
///
/// Each item carries both an `.action()` (for keyboard-shortcut display + global
/// key-binding dispatch) and an `.on_click()` handler (for the direct click
/// behaviour, which has access to local state like `has_selection` or the owning
/// panel entity). When the item is clicked the `on_click` handler runs; when the
/// global key binding fires, the action is dispatched to the focused element's
/// action chain, which reaches the `TerminalPanel`'s `on_action` handlers.
///
/// Layout for a terminal Space (with `split_ctx`):
/// 1. New Terminal
/// 2. Duplicate Session
/// 3. ── separator ──
/// 4. Split Right / Left / Up / Down
/// 4. ── separator ──
/// 5. Copy / Paste / Select All / Clear
/// 6. ── separator ──
/// 7. Close Terminal Tab
/// 8. Close Space (only when the tab has > 1 Space)
pub(crate) fn attach_context_menu<E>(
    div: E,
    session: Entity<Box<dyn TerminalSession>>,
    focus: FocusHandle,
    split_ctx: Option<SplitContext>,
) -> ContextMenu<E>
where
    E: gpui::InteractiveElement + gpui::ParentElement + gpui::Styled,
{
    div.context_menu({
        let session = session.clone();
        let focus = focus.clone();
        move |menu, window: &mut Window, cx| {
            let has_selection = session
                .read(cx)
                .selection_text()
                .map(|t| !t.is_empty())
                .unwrap_or(false);

            // 1. New Terminal — add a new TerminalPanel to the center dock.
            let mut menu = menu.item(
                PopupMenuItem::new("New Terminal")
                    .action(Box::new(AddPanel(oneterm_actions::DockPlacement::Center)))
                    .on_click({
                        let f = focus.clone();
                        move |_, window, cx| {
                            window.dispatch_action(
                                Box::new(AddPanel(oneterm_actions::DockPlacement::Center)),
                                cx,
                            );
                            window.focus(&f, cx);
                        }
                    }),
            );

            // 2. Duplicate Session — dynamic destinations for the right-clicked Space.
            if let Some(ctx) = split_ctx.clone() {
                let destinations = ctx
                    .panel
                    .upgrade()
                    .map(|panel| panel.read(cx).empty_space_destinations())
                    .unwrap_or_default();
                let submenu_ctx = ctx.clone();
                let submenu_focus = focus.clone();
                menu = menu.submenu("Duplicate Session", window, cx, move |submenu, _, _| {
                    let mut submenu = submenu.item(duplicate_item(
                        "In New Tab".into(),
                        DuplicateDestination::NewTab,
                        &submenu_ctx,
                        &submenu_focus,
                        true,
                    ));
                    for space_id in &destinations {
                        submenu = submenu.item(duplicate_item(
                            format!("Into Space #{}", space_id.display_number()),
                            DuplicateDestination::ExistingSpace(*space_id),
                            &submenu_ctx,
                            &submenu_focus,
                            false,
                        ));
                    }
                    submenu
                        .separator()
                        .item(duplicate_item(
                            "Split Right".into(),
                            DuplicateDestination::Split(SplitDir::Right),
                            &submenu_ctx,
                            &submenu_focus,
                            false,
                        ))
                        .item(duplicate_item(
                            "Split Down".into(),
                            DuplicateDestination::Split(SplitDir::Down),
                            &submenu_ctx,
                            &submenu_focus,
                            false,
                        ))
                });
            }

            // 3–4. Split Right / Left / Up / Down (only inside a Space tree).
            if let Some(ctx) = split_ctx.clone() {
                menu = menu
                    .separator()
                    .item(split_item("Split Right", SplitDir::Right, &ctx, &focus))
                    .item(split_item("Split Left", SplitDir::Left, &ctx, &focus))
                    .item(split_item("Split Up", SplitDir::Up, &ctx, &focus))
                    .item(split_item("Split Down", SplitDir::Down, &ctx, &focus));
            }

            // 4. ── separator ──
            menu = menu
                .separator()
                // 5. Copy
                .item(
                    PopupMenuItem::new("Copy")
                        .action(Box::new(TerminalCopy))
                        .disabled(!has_selection)
                        .on_click({
                            let s = session.clone();
                            let f = focus.clone();
                            move |_, window, cx| {
                                if let Some(text) = s.read(cx).selection_text() {
                                    if !text.is_empty() {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            text,
                                        ));
                                    }
                                }
                                window.focus(&f, cx);
                            }
                        }),
                )
                // 6. Paste
                .item(
                    PopupMenuItem::new("Paste")
                        .action(Box::new(TerminalPaste))
                        .on_click({
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
                        }),
                )
                // 7. Select All
                .item(
                    PopupMenuItem::new("Select All")
                        .action(Box::new(TerminalSelectAll))
                        .on_click({
                            let s = session.clone();
                            let f = focus.clone();
                            move |_, window, cx| {
                                s.update(cx, |s, _| s.select_all());
                                window.focus(&f, cx);
                            }
                        }),
                )
                // 8. Clear
                .item(
                    PopupMenuItem::new("Clear")
                        .action(Box::new(TerminalClear))
                        .on_click({
                            let s = session.clone();
                            let f = focus.clone();
                            move |_, window, cx| {
                                s.update(cx, |s, _| s.clear());
                                window.focus(&f, cx);
                            }
                        }),
                )
                // 9. ── separator ──
                .separator()
                // 10. Close Terminal Tab — dispatch the ClosePanel action.
                .item(
                    PopupMenuItem::new("Close Terminal Tab")
                        .action(Box::new(gpui_component::dock::ClosePanel))
                        .on_click({
                            let f = focus.clone();
                            move |_, window, cx| {
                                window.dispatch_action(
                                    Box::new(gpui_component::dock::ClosePanel),
                                    cx,
                                );
                                window.focus(&f, cx);
                            }
                        }),
                );

            // 11. Close Space — directly below Close Terminal Tab, only when the
            // tab has more than one Space.
            if let Some(ctx) = split_ctx.clone() {
                let can_close_space = ctx
                    .panel
                    .upgrade()
                    .map(|p| p.read(cx).leaf_count() > 1)
                    .unwrap_or(false);
                if can_close_space {
                    menu = menu.item(
                        PopupMenuItem::new("Close Space")
                            .action(Box::new(CloseSpace))
                            .on_click({
                                let f = focus.clone();
                                let panel = ctx.panel.clone();
                                let space_id = ctx.space_id;
                                move |_, window, cx| {
                                    if let Some(panel) = panel.upgrade() {
                                        panel.update(cx, |p, cx| {
                                            p.close_space(space_id, window, cx);
                                        });
                                    }
                                    window.focus(&f, cx);
                                }
                            }),
                    );
                }
            }

            menu
        }
    })
}

/// Build one Duplicate Session destination item.
fn duplicate_item(
    label: String,
    destination: DuplicateDestination,
    ctx: &SplitContext,
    focus: &FocusHandle,
    show_action: bool,
) -> PopupMenuItem {
    let panel = ctx.panel.clone();
    let source_space = ctx.space_id;
    let f = focus.clone();
    let item = PopupMenuItem::new(label).on_click(move |_, window, cx| {
        if let Some(panel) = panel.upgrade() {
            panel.update(cx, |panel, cx| {
                panel.duplicate_session_to(source_space, destination, window, cx);
            });
        }
        window.focus(&f, cx);
    });
    if show_action {
        item.action(Box::new(DuplicateSession))
    } else {
        item
    }
}

/// Build a "Split <dir>" menu item that dispatches to the owning panel.
fn split_item(
    label: &'static str,
    dir: SplitDir,
    ctx: &SplitContext,
    focus: &FocusHandle,
) -> PopupMenuItem {
    let panel = ctx.panel.clone();
    let space_id = ctx.space_id;
    let f = focus.clone();
    PopupMenuItem::new(label)
        .action(match dir {
            SplitDir::Right => Box::new(SplitRight),
            SplitDir::Left => Box::new(SplitLeft),
            SplitDir::Up => Box::new(SplitUp),
            SplitDir::Down => Box::new(SplitDown),
        })
        .on_click(move |_, window, cx| {
            if let Some(panel) = panel.upgrade() {
                panel.update(cx, |p, cx| p.split_active_at(space_id, dir, window, cx));
            }
            window.focus(&f, cx);
        })
}
