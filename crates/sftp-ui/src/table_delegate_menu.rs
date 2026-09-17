//! The SFTP browser's single action list, and the menus built from it.
//!
//! `F30` (IN-0042): the toolbar's `⋮` menu and the table's right-click menu
//! used to be two hand-written lists that had drifted apart in contents, order
//! and icons — only the right-click menu offered Edit and Refresh. Both now
//! render [`ENTRY_MENU`], so an item added to one is in the other by
//! construction; the empty-area menu renders the subset of that list that needs
//! no selection.
//!
//! Each item carries a `.action()` (so the keyboard shortcut is displayed) and
//! an `.on_click()` that runs the matching [`SftpPanel`] method right away —
//! `PopupMenuItem::on_click` receives the `&mut Window`, so no work is deferred
//! into the render pass.

use gpui::{App, ClickEvent, Context, Window};
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::{Icon, IconName};
use oneterm_theme::icon::AppIcon;

use super::panel::SftpPanel;

/// One action of the SFTP browser, as offered by its menus.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SftpAction {
    Open,
    Edit,
    Download,
    UploadFiles,
    UploadFolder,
    NewFolder,
    Rename,
    Delete,
    Properties,
    Refresh,
}

/// A row of a menu: an action or a separator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MenuEntry {
    Action(SftpAction),
    Separator,
}

/// The canonical action list. Both the `⋮` menu and the right-click menu render
/// exactly this, in this order.
pub(crate) const ENTRY_MENU: &[MenuEntry] = &[
    MenuEntry::Action(SftpAction::Open),
    MenuEntry::Action(SftpAction::Edit),
    MenuEntry::Action(SftpAction::Download),
    MenuEntry::Separator,
    MenuEntry::Action(SftpAction::UploadFiles),
    MenuEntry::Action(SftpAction::UploadFolder),
    MenuEntry::Action(SftpAction::NewFolder),
    MenuEntry::Separator,
    MenuEntry::Action(SftpAction::Rename),
    MenuEntry::Action(SftpAction::Delete),
    MenuEntry::Separator,
    MenuEntry::Action(SftpAction::Properties),
    MenuEntry::Action(SftpAction::Refresh),
];

impl SftpAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            SftpAction::Open => "Open",
            SftpAction::Edit => "Edit",
            SftpAction::Download => "Download",
            SftpAction::UploadFiles => "Upload Files",
            SftpAction::UploadFolder => "Upload Folder",
            SftpAction::NewFolder => "New Folder",
            SftpAction::Rename => "Rename",
            SftpAction::Delete => "Delete",
            SftpAction::Properties => "Properties",
            SftpAction::Refresh => "Refresh",
        }
    }

    fn icon(self) -> Icon {
        match self {
            SftpAction::Open => Icon::new(IconName::FolderOpen),
            SftpAction::Edit => Icon::new(IconName::FileText),
            SftpAction::Download => Icon::new(IconName::ArrowDown),
            SftpAction::UploadFiles | SftpAction::UploadFolder => Icon::new(IconName::ArrowUp),
            SftpAction::NewFolder => Icon::new(IconName::Plus),
            SftpAction::Rename => Icon::new(IconName::Replace),
            SftpAction::Delete => Icon::new(IconName::Delete),
            SftpAction::Properties => Icon::new(IconName::Info),
            SftpAction::Refresh => Icon::new(AppIcon::Refresh),
        }
    }

    /// The bound action, so the menu row can show its keyboard shortcut.
    fn keyboard_action(self) -> Box<dyn gpui::Action> {
        match self {
            SftpAction::Open => Box::new(oneterm_actions::SftpOpen),
            SftpAction::Edit => Box::new(oneterm_actions::SftpEdit),
            SftpAction::Download => Box::new(oneterm_actions::SftpDownload),
            SftpAction::UploadFiles => Box::new(oneterm_actions::SftpUploadFiles),
            SftpAction::UploadFolder => Box::new(oneterm_actions::SftpUploadFolder),
            SftpAction::NewFolder => Box::new(oneterm_actions::SftpNewFolder),
            SftpAction::Rename => Box::new(oneterm_actions::SftpRename),
            SftpAction::Delete => Box::new(oneterm_actions::SftpDelete),
            SftpAction::Properties => Box::new(oneterm_actions::SftpProperties),
            SftpAction::Refresh => Box::new(oneterm_actions::SftpRefresh),
        }
    }

    /// Whether the action acts on the selected entry. The others work on the
    /// current directory and are the ones the empty-area menu keeps.
    pub(crate) fn needs_selection(self) -> bool {
        match self {
            SftpAction::Open
            | SftpAction::Edit
            | SftpAction::Download
            | SftpAction::Rename
            | SftpAction::Delete
            | SftpAction::Properties => true,
            SftpAction::UploadFiles
            | SftpAction::UploadFolder
            | SftpAction::NewFolder
            | SftpAction::Refresh => false,
        }
    }

    fn run(self, panel: &mut SftpPanel, window: &mut Window, cx: &mut Context<SftpPanel>) {
        match self {
            SftpAction::Open => panel.do_open(window, cx),
            SftpAction::Edit => panel.do_edit(window, cx),
            SftpAction::Download => panel.do_download(window, cx),
            SftpAction::UploadFiles => panel.do_upload(false, window, cx),
            SftpAction::UploadFolder => panel.do_upload(true, window, cx),
            SftpAction::NewFolder => panel.do_new_folder(window, cx),
            SftpAction::Rename => panel.do_rename(window, cx),
            SftpAction::Delete => panel.do_delete(window, cx),
            SftpAction::Properties => panel.do_properties(window, cx),
            SftpAction::Refresh => panel.refresh(cx),
        }
    }
}

/// The empty-area menu: [`ENTRY_MENU`] without the actions that need a
/// selection, and without the separators that would be left dangling.
pub(crate) fn empty_area_entries() -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = Vec::new();
    for entry in ENTRY_MENU {
        match entry {
            MenuEntry::Action(action) if action.needs_selection() => {}
            MenuEntry::Separator => {
                // Never lead with a separator, never repeat one.
                if matches!(entries.last(), Some(MenuEntry::Action(_))) {
                    entries.push(MenuEntry::Separator);
                }
            }
            entry => entries.push(*entry),
        }
    }
    while matches!(entries.last(), Some(MenuEntry::Separator)) {
        entries.pop();
    }
    entries
}

/// Render `entries` onto `menu`. The one renderer both menus use.
pub(super) fn build_menu(
    mut menu: PopupMenu,
    panel: &gpui::WeakEntity<SftpPanel>,
    entries: &[MenuEntry],
) -> PopupMenu {
    for entry in entries {
        menu = match *entry {
            MenuEntry::Separator => menu.separator(),
            MenuEntry::Action(action) => menu.item(
                PopupMenuItem::new(action.label())
                    .icon(action.icon())
                    .action(action.keyboard_action())
                    .on_click(on_click_entity(panel.clone(), move |this, window, cx| {
                        action.run(this, window, cx)
                    })),
            ),
        };
    }
    menu
}

/// Create an `on_click` closure that runs `action` on `entity` (the panel or
/// the local pane, if it is still alive) with the window the click arrived on.
pub(super) fn on_click_entity<T: 'static>(
    entity: gpui::WeakEntity<T>,
    action: impl Fn(&mut T, &mut Window, &mut gpui::Context<T>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| {
        if let Some(entity) = entity.upgrade() {
            entity.update(cx, |this, cx| action(this, window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[MenuEntry]) -> Vec<&'static str> {
        entries
            .iter()
            .map(|entry| match entry {
                MenuEntry::Action(action) => action.label(),
                MenuEntry::Separator => "---",
            })
            .collect()
    }

    /// F30: one list feeds both menus, so this is the order both of them show —
    /// Edit and Refresh included, which only the right-click menu used to have.
    #[test]
    fn the_shared_action_list_has_one_order() {
        assert_eq!(
            labels(ENTRY_MENU),
            vec![
                "Open",
                "Edit",
                "Download",
                "---",
                "Upload Files",
                "Upload Folder",
                "New Folder",
                "---",
                "Rename",
                "Delete",
                "---",
                "Properties",
                "Refresh",
            ]
        );
    }

    /// The empty-area menu is a filtered view of the same list: same relative
    /// order, no action that needs a selection, no dangling separator.
    #[test]
    fn the_empty_area_menu_is_a_subset_of_the_same_list() {
        let entries = empty_area_entries();
        assert_eq!(
            labels(&entries),
            vec![
                "Upload Files",
                "Upload Folder",
                "New Folder",
                "---",
                "Refresh"
            ]
        );
        assert!(!matches!(entries.first(), Some(MenuEntry::Separator)));
        assert!(!matches!(entries.last(), Some(MenuEntry::Separator)));
        assert!(
            entries
                .windows(2)
                .all(|pair| pair != [MenuEntry::Separator, MenuEntry::Separator])
        );
        // Every kept action is one that works on the current directory.
        for entry in &entries {
            if let MenuEntry::Action(action) = entry {
                assert!(!action.needs_selection(), "{action:?} needs a selection");
            }
        }
    }
}
