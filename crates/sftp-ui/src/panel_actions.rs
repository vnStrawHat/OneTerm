//! SFTP context-menu action handlers for [`SftpPanel`].
//!
//! Thin wrappers that map gpui Action types (fired by the context menu via
//! `.action()` or by global key bindings via `.on_action`) to the existing
//! `do_*` methods defined in [`super::actions`] and [`super::transfer`].

use gpui::{Context, Window};

use super::panel::SftpPanel;

impl SftpPanel {
    /// Action handler: open the selected entry (navigate into dir or download file).
    pub(crate) fn on_action_sftp_open(
        &mut self,
        _: &oneterm_actions::SftpOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.browser().selected() {
            if let Some(entry) = self.selected_entry(cx) {
                if entry.is_dir {
                    self.navigate_into(ix, cx);
                } else {
                    self.do_download(window, cx);
                }
            }
        }
    }

    /// Action handler: download the selected file/folder.
    pub(crate) fn on_action_sftp_download(
        &mut self,
        _: &oneterm_actions::SftpDownload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_download(window, cx);
    }

    /// Action handler: rename the selected entry.
    pub(crate) fn on_action_sftp_rename(
        &mut self,
        _: &oneterm_actions::SftpRename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_rename(window, cx);
    }

    /// Action handler: delete the selected entry.
    pub(crate) fn on_action_sftp_delete(
        &mut self,
        _: &oneterm_actions::SftpDelete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_delete(window, cx);
    }

    /// Action handler: show properties of the selected entry.
    pub(crate) fn on_action_sftp_properties(
        &mut self,
        _: &oneterm_actions::SftpProperties,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_properties(window, cx);
    }

    /// Action handler: upload files.
    pub(crate) fn on_action_sftp_upload_files(
        &mut self,
        _: &oneterm_actions::SftpUploadFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_upload(false, window, cx);
    }

    /// Action handler: upload a folder.
    pub(crate) fn on_action_sftp_upload_folder(
        &mut self,
        _: &oneterm_actions::SftpUploadFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_upload(true, window, cx);
    }

    /// Action handler: create a new folder.
    pub(crate) fn on_action_sftp_new_folder(
        &mut self,
        _: &oneterm_actions::SftpNewFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_new_folder(window, cx);
    }

    /// Action handler: refresh the file listing.
    pub(crate) fn on_action_sftp_refresh(
        &mut self,
        _: &oneterm_actions::SftpRefresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh(cx);
    }
}
