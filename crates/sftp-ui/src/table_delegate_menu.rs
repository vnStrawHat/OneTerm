//! Context-menu builders for the SFTP table.
//!
//! Both the row context menu and the empty-area context menu are built here.
//! Each menu item carries a `.action()` (for keyboard-shortcut display) and an
//! `.on_click()` that runs the matching [`SftpPanel`] method right away —
//! `PopupMenuItem::on_click` receives the `&mut Window`, so no work is deferred
//! into the render pass.

use gpui::{App, ClickEvent, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};

use super::panel::SftpPanel;

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
                .action(Box::new(oneterm_actions::SftpOpen))
                .on_click(on_click_panel(panel.clone(), move |this, _, cx| {
                    this.navigate_into(row_ix, cx)
                })),
        )
        .item(
            PopupMenuItem::new("Download")
                .action(Box::new(oneterm_actions::SftpDownload))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_download)),
        )
    } else {
        menu.item(
            PopupMenuItem::new("Edit")
                .action(Box::new(oneterm_actions::SftpEdit))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_edit)),
        )
        .item(
            PopupMenuItem::new("Download")
                .action(Box::new(oneterm_actions::SftpDownload))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_download)),
        )
    };

    // Shared items: Rename, Delete.
    menu.separator()
        .item(
            PopupMenuItem::new("Rename")
                .action(Box::new(oneterm_actions::SftpRename))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_rename)),
        )
        .item(
            PopupMenuItem::new("Delete")
                .action(Box::new(oneterm_actions::SftpDelete))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_delete)),
        )
        // Properties.
        .separator()
        .item(
            PopupMenuItem::new("Properties")
                .action(Box::new(oneterm_actions::SftpProperties))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_properties)),
        )
        // Upload + New Folder + Refresh.
        .separator()
        .item(
            PopupMenuItem::new("Upload Files")
                .action(Box::new(oneterm_actions::SftpUploadFiles))
                .on_click(on_click_panel(panel.clone(), |this, window, cx| {
                    this.do_upload(false, window, cx)
                })),
        )
        .item(
            PopupMenuItem::new("Upload Folder")
                .action(Box::new(oneterm_actions::SftpUploadFolder))
                .on_click(on_click_panel(panel.clone(), |this, window, cx| {
                    this.do_upload(true, window, cx)
                })),
        )
        .item(
            PopupMenuItem::new("New Folder")
                .action(Box::new(oneterm_actions::SftpNewFolder))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_new_folder)),
        )
        .item(
            PopupMenuItem::new("Refresh")
                .action(Box::new(oneterm_actions::SftpRefresh))
                .on_click(on_click_panel(panel.clone(), |this, _, cx| {
                    this.refresh(cx)
                })),
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
                .action(Box::new(oneterm_actions::SftpUploadFiles))
                .on_click(on_click_panel(panel.clone(), |this, window, cx| {
                    this.do_upload(false, window, cx)
                })),
        )
        .item(
            PopupMenuItem::new("Upload Folder")
                .action(Box::new(oneterm_actions::SftpUploadFolder))
                .on_click(on_click_panel(panel.clone(), |this, window, cx| {
                    this.do_upload(true, window, cx)
                })),
        )
        .item(
            PopupMenuItem::new("New Folder")
                .action(Box::new(oneterm_actions::SftpNewFolder))
                .on_click(on_click_panel(panel.clone(), SftpPanel::do_new_folder)),
        )
        .separator()
        .item(
            PopupMenuItem::new("Refresh")
                .action(Box::new(oneterm_actions::SftpRefresh))
                .on_click(on_click_panel(panel.clone(), |this, _, cx| {
                    this.refresh(cx)
                })),
        );
    menu
}

/// Create an `on_click` closure that runs `action` on the panel (if it is
/// still alive) with the window the click arrived on.
pub(super) fn on_click_panel(
    panel: gpui::WeakEntity<SftpPanel>,
    action: impl Fn(&mut SftpPanel, &mut Window, &mut gpui::Context<SftpPanel>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        if let Some(panel) = panel.upgrade() {
            panel.update(cx, |this, cx| action(this, window, cx));
        }
    }
}
