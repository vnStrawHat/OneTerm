//! Context menu for `LocalTerminalView`.

use gpui::{Entity, FocusHandle, WeakEntity, Window};
use gpui_component::menu::{ContextMenu, ContextMenuExt as _, PopupMenu, PopupMenuItem};

use oneterm_terminal::TerminalSession;

use oneterm_actions::{
    AddPanel, CloseSpace, DuplicateSession, SplitDown, SplitLeft, SplitRight, SplitUp,
    TerminalClear, TerminalCopy, TerminalPaste, TerminalSelectAll,
};

use super::super::panel::{DuplicateDestination, TerminalPanel};
use super::super::space::{SpaceId, SplitContext, SplitDir};
use super::edit;

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
            let has_selection = session.read(cx).has_selection();

            // 1. New Terminal — add a new TerminalPanel to the center dock.
            let mut menu = menu.item(
                PopupMenuItem::new("New Terminal")
                    .action(Box::new(AddPanel))
                    .on_click({
                        let f = focus.clone();
                        move |_, window, cx| {
                            window.dispatch_action(Box::new(AddPanel), cx);
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
                menu = split_items(
                    menu.separator(),
                    ctx.panel.clone(),
                    ctx.space_id,
                    Some(&focus),
                );
            }

            // 4. ── separator ──
            menu = menu
                .separator()
                // 5. Copy
                .item(
                    edit_item(
                        "Copy",
                        Box::new(TerminalCopy),
                        &session,
                        &focus,
                        edit::copy_selection,
                    )
                    .disabled(!has_selection),
                )
                // 6. Paste
                .item(edit_item(
                    "Paste",
                    Box::new(TerminalPaste),
                    &session,
                    &focus,
                    edit::paste_clipboard,
                ))
                // 7. Select All
                .item(edit_item(
                    "Select All",
                    Box::new(TerminalSelectAll),
                    &session,
                    &focus,
                    edit::select_all,
                ))
                // 8. Clear
                .item(edit_item(
                    "Clear",
                    Box::new(TerminalClear),
                    &session,
                    &focus,
                    edit::clear_screen,
                ))
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

/// Build one Copy / Paste / Select All / Clear item: the action (for the
/// shortcut hint + global dispatch) plus a click handler that runs `edit` on
/// the session and returns focus to the terminal.
fn edit_item(
    label: &'static str,
    action: Box<dyn gpui::Action>,
    session: &Entity<Box<dyn TerminalSession>>,
    focus: &FocusHandle,
    edit: edit::EditCommand,
) -> PopupMenuItem {
    let s = session.clone();
    let f = focus.clone();
    PopupMenuItem::new(label)
        .action(action)
        .on_click(move |_, window, cx| {
            edit(&s, window, cx);
            window.focus(&f, cx);
        })
}

/// Append the four "Split Right / Left / Up / Down" items that split
/// `space_id` of `panel`. `focus` (the terminal's handle, when the menu belongs
/// to a terminal Space) is re-focused after the click.
pub(crate) fn split_items(
    menu: PopupMenu,
    panel: WeakEntity<TerminalPanel>,
    space_id: SpaceId,
    focus: Option<&FocusHandle>,
) -> PopupMenu {
    let item = |label: &'static str, dir: SplitDir| {
        let panel = panel.clone();
        let f = focus.cloned();
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
                if let Some(f) = &f {
                    window.focus(f, cx);
                }
            })
    };
    menu.item(item("Split Right", SplitDir::Right))
        .item(item("Split Left", SplitDir::Left))
        .item(item("Split Up", SplitDir::Up))
        .item(item("Split Down", SplitDir::Down))
}
