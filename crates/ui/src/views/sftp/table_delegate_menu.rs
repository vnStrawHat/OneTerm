//! Context-menu builders for the SFTP table — extracted from [`super`] to keep
//! the delegate file under the ~400-line guideline.
//!
//! Both the row context menu and the empty-area context menu are built here.
//! Each menu item carries a `.action()` (for keyboard-shortcut display) and an
//! `.on_click()` (which sets `pending_action` on [`SftpPanel`]).

use gpui::{App, ClickEvent, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use super::panel::SftpPanel;
use super::types::PendingAction;

/// Build the context menu for a file/folder entry at `row_ix`.
///
/// Called from `SftpTableDelegate::context_menu` after the row has been selected
/// and the entry looked up. The first section is conditional on `is_dir`
/// (Open + Download for dirs, Download only for files); the rest is shared.
pub(super) fn build_entry_menu(
    menu: PopupMenu,
    panel: &gpui::WeakEntity<SftpPanel>,
    row_ix: usize,
    is_dir: bool,
) -> PopupMenu {
    // First section: Open (dir only) + Download.
    let menu = if is_dir {
        menu.item(
            PopupMenuItem::new("Open")
                .action(Box::new(crate::actions::SftpOpen))
                .on_click(on_click_pending(panel.clone(), PendingAction::Open(row_ix))),
        )
        .item(
            PopupMenuItem::new("Download")
                .action(Box::new(crate::actions::SftpDownload))
                .on_click(on_click_pending(panel.clone(), PendingAction::Download)),
        )
    } else {
        menu.item(
            PopupMenuItem::new("Download")
                .action(Box::new(crate::actions::SftpDownload))
                .on_click(on_click_pending(panel.clone(), PendingAction::Download)),
        )
    };

    // Shared items: Rename, Delete.
    menu.separator()
        .item(
            PopupMenuItem::new("Rename")
                .action(Box::new(crate::actions::SftpRename))
                .on_click(on_click_pending(panel.clone(), PendingAction::Rename)),
        )
        .item(
            PopupMenuItem::new("Delete")
                .action(Box::new(crate::actions::SftpDelete))
                .on_click(on_click_pending(panel.clone(), PendingAction::Delete)),
        )
        // Properties.
        .separator()
        .item(
            PopupMenuItem::new("Properties")
                .action(Box::new(crate::actions::SftpProperties))
                .on_click(on_click_pending(panel.clone(), PendingAction::Properties)),
        )
        // Upload + New Folder + Refresh.
        .separator()
        .item(
            PopupMenuItem::new("Upload Files")
                .action(Box::new(crate::actions::SftpUploadFiles))
                .on_click(on_click_pending(panel.clone(), PendingAction::UploadFiles)),
        )
        .item(
            PopupMenuItem::new("Upload Folder")
                .action(Box::new(crate::actions::SftpUploadFolder))
                .on_click(on_click_pending(panel.clone(), PendingAction::UploadFolder)),
        )
        .item(
            PopupMenuItem::new("New Folder")
                .action(Box::new(crate::actions::SftpNewFolder))
                .on_click(on_click_pending(panel.clone(), PendingAction::NewFolder)),
        )
        .item(
            PopupMenuItem::new("Refresh")
                .action(Box::new(crate::actions::SftpRefresh))
                .on_click(on_click_pending(panel.clone(), PendingAction::Refresh)),
        )
}

/// Build the empty-area context menu items on a `PopupMenu`.
///
/// Called from the `context_menu` closure in `render_empty`.
/// Only shows upload/new-folder/refresh (no row-specific actions).
pub(super) fn build_empty_menu(
    mut menu: PopupMenu,
    panel: &gpui::WeakEntity<SftpPanel>,
) -> PopupMenu {
    menu = menu
        .item(
            PopupMenuItem::new("Upload Files")
                .action(Box::new(crate::actions::SftpUploadFiles))
                .on_click(on_click_pending(panel.clone(), PendingAction::UploadFiles)),
        )
        .item(
            PopupMenuItem::new("Upload Folder")
                .action(Box::new(crate::actions::SftpUploadFolder))
                .on_click(on_click_pending(panel.clone(), PendingAction::UploadFolder)),
        )
        .item(
            PopupMenuItem::new("New Folder")
                .action(Box::new(crate::actions::SftpNewFolder))
                .on_click(on_click_pending(panel.clone(), PendingAction::NewFolder)),
        )
        .separator()
        .item(
            PopupMenuItem::new("Refresh")
                .action(Box::new(crate::actions::SftpRefresh))
                .on_click(on_click_pending(panel.clone(), PendingAction::Refresh)),
        );
    menu
}

/// Create an `on_click` closure that sets `pending_action` on the panel.
fn on_click_pending(
    panel: gpui::WeakEntity<SftpPanel>,
    action: PendingAction,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| {
        if let Some(panel) = panel.upgrade() {
            panel.update(cx, |this, cx| {
                this.pending_action = Some(action);
                cx.notify();
            });
        }
    }
}
