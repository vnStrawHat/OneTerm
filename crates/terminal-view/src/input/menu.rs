//! The terminal's right-click menu.
//!
//! [`build_menu`] takes a [`MenuContext`] instead of a view reference: it needs
//! a handful of read-only facts (selection, split destinations, logging), and
//! taking them as data keeps the builder callable from the wrapper's
//! `context_menu` closure without borrowing the view across the call.

use gpui::{App, Entity, FocusHandle, NoAction, WeakEntity, Window};
use gpui_component::{
    WindowExt as _,
    menu::{PopupMenu, PopupMenuItem},
    notification::NotificationType,
};

use oneterm_actions::{
    AddPanel, CloseSpace, DuplicateSession, SplitDown, SplitLeft, SplitRight, SplitUp,
    TerminalClear, TerminalCopy, TerminalPaste, TerminalSelectAll,
};
use oneterm_settings::TerminalSettings;
use oneterm_terminal::{TerminalLogController, TerminalLogState, TerminalSession};
use oneterm_theme::notif_ext::notify;

use super::edit;
use crate::panel::{DuplicateDestination, TerminalPanel};
use crate::space::{SpaceId, SplitContext, SplitDir};
use crate::terminal_view::BroadcastOrigin;

/// What the menu builder needs to know about the terminal it belongs to.
pub(crate) struct MenuContext {
    pub session: Entity<Box<dyn TerminalSession>>,
    /// The terminal the menu belongs to — a paste from here reaches its
    /// broadcast channel peers like any other input.
    pub origin: BroadcastOrigin,
    /// Re-focused after every item so typing continues in the terminal.
    pub focus: FocusHandle,
    /// Enables the Copy item.
    pub has_selection: bool,
    /// `None` for a terminal that is not inside a Space tree: no Duplicate
    /// submenu, no Split items, no Close Space.
    pub split: Option<MenuSplitContext>,
    /// `capabilities().logging` — the Log submenu exists only with it.
    pub logging: Option<TerminalLogController>,
}

/// The Space-tree facts the menu needs, read once by the caller.
pub(crate) struct MenuSplitContext {
    pub ctx: SplitContext,
    /// Spaces in this tab that hold no terminal — Duplicate destinations.
    pub empty_destinations: Vec<SpaceId>,
    /// Leaves in the tab; Close Space needs a sibling to close into.
    pub leaf_count: usize,
}

/// Whether the "Close Space" item is offered: closing the only Space would
/// leave the tab empty, which is what "Close Terminal Tab" is for.
pub(crate) fn can_close_space(leaf_count: usize) -> bool {
    leaf_count > 1
}

/// Label of one Duplicate Session destination.
pub(crate) fn duplicate_label(destination: DuplicateDestination) -> String {
    match destination {
        DuplicateDestination::NewTab => "In New Tab".to_string(),
        DuplicateDestination::ExistingSpace(id) => {
            format!("Into Space #{}", id.display_number())
        }
        DuplicateDestination::Split(SplitDir::Right) => "Split Right".to_string(),
        DuplicateDestination::Split(SplitDir::Left) => "Split Left".to_string(),
        DuplicateDestination::Split(SplitDir::Up) => "Split Up".to_string(),
        DuplicateDestination::Split(SplitDir::Down) => "Split Down".to_string(),
    }
}

/// Build the menu in the order fixed by the LLD: New Terminal, Duplicate
/// Session, splits, edit commands, Log, close items.
///
/// Each item carries both an action (for the shortcut hint and global
/// dispatch) and a click handler (which has the local state a global action
/// cannot see, such as which Space was right-clicked).
pub(crate) fn build_menu(
    menu: PopupMenu,
    ctx: &MenuContext,
    window: &mut Window,
    cx: &mut gpui::Context<PopupMenu>,
) -> PopupMenu {
    let focus = ctx.focus.clone();
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

    if let Some(split) = &ctx.split {
        let destinations = split.empty_destinations.clone();
        let submenu_ctx = split.ctx.clone();
        let submenu_focus = focus.clone();
        menu = menu.submenu("Duplicate Session", window, cx, move |submenu, _, _| {
            let mut submenu = submenu.item(duplicate_item(
                DuplicateDestination::NewTab,
                &submenu_ctx,
                &submenu_focus,
                true,
            ));
            for space_id in &destinations {
                submenu = submenu.item(duplicate_item(
                    DuplicateDestination::ExistingSpace(*space_id),
                    &submenu_ctx,
                    &submenu_focus,
                    false,
                ));
            }
            submenu
                .separator()
                .item(duplicate_item(
                    DuplicateDestination::Split(SplitDir::Right),
                    &submenu_ctx,
                    &submenu_focus,
                    false,
                ))
                .item(duplicate_item(
                    DuplicateDestination::Split(SplitDir::Down),
                    &submenu_ctx,
                    &submenu_focus,
                    false,
                ))
        });
        menu = split_items(
            menu.separator(),
            split.ctx.panel.clone(),
            split.ctx.space_id,
            Some(&focus),
        );
    }

    menu = menu
        .separator()
        .item(
            edit_item(
                "Copy",
                Box::new(TerminalCopy),
                ctx,
                edit::copy_selection as edit::EditCommand,
            )
            .disabled(!ctx.has_selection),
        )
        .item(edit_item(
            "Paste",
            Box::new(TerminalPaste),
            ctx,
            edit::paste_clipboard,
        ))
        .item(edit_item(
            "Select All",
            Box::new(TerminalSelectAll),
            ctx,
            edit::select_all,
        ))
        .item(edit_item(
            "Clear",
            Box::new(TerminalClear),
            ctx,
            edit::clear_screen,
        ))
        .separator();

    if let Some(logging) = ctx.logging.clone() {
        let running = matches!(logging.state(), TerminalLogState::Running { .. });
        menu = menu
            .submenu("Log", window, cx, move |submenu, _, _| {
                submenu
                    .item(
                        PopupMenuItem::new("Start")
                            .action(Box::new(NoAction {}))
                            .disabled(running)
                            .on_click({
                                let logging = logging.clone();
                                move |_, window, cx| start_logging(logging.clone(), window, cx)
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Stop")
                            .action(Box::new(NoAction {}))
                            .disabled(!running)
                            .on_click({
                                let logging = logging.clone();
                                move |_, window, cx| stop_logging(logging.clone(), window, cx)
                            }),
                    )
            })
            .separator();
    }

    menu = menu.item(
        PopupMenuItem::new("Close Terminal Tab")
            .action(Box::new(gpui_component::dock::ClosePanel))
            .on_click({
                let f = focus.clone();
                move |_, window, cx| {
                    window.dispatch_action(Box::new(gpui_component::dock::ClosePanel), cx);
                    window.focus(&f, cx);
                }
            }),
    );

    if let Some(split) = &ctx.split
        && can_close_space(split.leaf_count)
    {
        let panel = split.ctx.panel.clone();
        let space_id = split.ctx.space_id;
        menu = menu.item(
            PopupMenuItem::new("Close Space")
                .action(Box::new(CloseSpace))
                .on_click({
                    let f = focus.clone();
                    move |_, window, cx| {
                        if let Some(panel) = panel.upgrade() {
                            panel.update(cx, |p, cx| p.close_space(space_id, window, cx));
                        }
                        window.focus(&f, cx);
                    }
                }),
        );
    }

    menu
}

/// One Duplicate Session destination. Only the first item carries the
/// `DuplicateSession` action, so the shortcut hint is shown once.
fn duplicate_item(
    destination: DuplicateDestination,
    ctx: &SplitContext,
    focus: &FocusHandle,
    show_action: bool,
) -> PopupMenuItem {
    let panel = ctx.panel.clone();
    let source_space = ctx.space_id;
    let f = focus.clone();
    let item = PopupMenuItem::new(duplicate_label(destination)).on_click(move |_, window, cx| {
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

/// The four "Split …" items that split `space_id` of `panel`; shared with the
/// empty-Space placeholder menu. `focus` (the terminal's handle, when the menu
/// belongs to a terminal Space) is re-focused after the click.
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
                SplitDir::Right => Box::new(SplitRight) as Box<dyn gpui::Action>,
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

/// One edit command item: the action for the shortcut hint plus a click
/// handler that runs the command and returns focus to the terminal.
fn edit_item(
    label: &'static str,
    action: Box<dyn gpui::Action>,
    ctx: &MenuContext,
    command: edit::EditCommand,
) -> PopupMenuItem {
    let session = ctx.session.clone();
    let origin = ctx.origin.clone();
    let f = ctx.focus.clone();
    PopupMenuItem::new(label)
        .action(action)
        .on_click(move |_, window, cx| {
            command(&session, &origin, window, cx);
            window.focus(&f, cx);
        })
}

/// Start logging this terminal. The file is opened on the background executor
/// because it touches the filesystem.
fn start_logging(logging: TerminalLogController, window: &mut Window, cx: &mut App) {
    let config = TerminalSettings::global(cx)
        .read(cx)
        .logging
        .ssh_config(true);
    window
        .spawn(cx, async move |cx| {
            let operation = logging.clone();
            let result = cx
                .background_executor()
                .spawn(async move { operation.start(&config) })
                .await;
            _ = cx.update(|window, cx| {
                match result {
                    Ok(path) => window.push_notification(
                        notify(
                            NotificationType::Success,
                            format!("Logging to {}", path.display()),
                            cx,
                        ),
                        cx,
                    ),
                    Err(error) => {
                        // The controller keeps the error for its own state
                        // machine; the toast is the user-facing report.
                        _ = logging.take_error();
                        window.push_notification(
                            notify(NotificationType::Error, error.to_string(), cx),
                            cx,
                        )
                    }
                }
                window.refresh();
            });
        })
        .detach();
}

fn stop_logging(logging: TerminalLogController, window: &mut Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            let operation = logging.clone();
            let result = cx
                .background_executor()
                .spawn(async move { operation.stop() })
                .await;
            _ = cx.update(|window, cx| {
                match result {
                    Ok(()) => window.push_notification(
                        notify(NotificationType::Success, "Terminal logging stopped.", cx),
                        cx,
                    ),
                    Err(error) => {
                        _ = logging.take_error();
                        window.push_notification(
                            notify(NotificationType::Error, error.to_string(), cx),
                            cx,
                        )
                    }
                }
                window.refresh();
            });
        })
        .detach();
}
