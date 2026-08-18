//! File operations for the SFTP browser — rename, delete, new folder, properties.
//!
//! These methods open a dialog (input/confirm) and then call the SFTP backend.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, AppContext, Context, Entity, ParentElement, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    v_flex,
};
use oneterm_core::{AppError, FileEntry, RemotePath, SftpBackend, SftpStatus};
use oneterm_state::form_dialog::{FieldRequirement, FormDialog, labelled_field};
use oneterm_state::notif_ext::notify;

use super::panel::SftpPanel;
use super::types::{format_date, format_owner, format_permissions, format_size};

/// Why a typed entry name was rejected. `Display` is the corrective message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryNameError {
    Empty,
    /// `/` would address another directory instead of naming an entry.
    ContainsSlash,
    /// `.` / `..` are the current and parent directory, never a new entry.
    DotOrDotDot,
}

impl std::fmt::Display for EntryNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Name cannot be empty."),
            Self::ContainsSlash => f.write_str("Name cannot contain '/'."),
            Self::DotOrDotDot => f.write_str("Name cannot be '.' or '..'."),
        }
    }
}

/// Validate a single path component typed into the rename / new-folder
/// dialogs (CORR-53): the trimmed name must be non-empty, contain no `/`, and
/// not be `.` or `..`.
fn validate_entry_name(name: &str) -> Result<&str, EntryNameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(EntryNameError::Empty);
    }
    if name.contains('/') {
        return Err(EntryNameError::ContainsSlash);
    }
    if name == "." || name == ".." {
        return Err(EntryNameError::DotOrDotDot);
    }
    Ok(name)
}

/// The remote path an entry gets when it is renamed to `new_name` in place.
fn rename_target(from: &RemotePath, new_name: &str) -> RemotePath {
    from.parent()
        .unwrap_or_else(RemotePath::root)
        .join(new_name)
}

impl SftpPanel {
    /// Return the active SFTP backend, or notify the user and yield `None` when
    /// none is available.
    ///
    /// These actions are only reachable while a backend is present, so `None`
    /// reflects a race with a disconnect rather than normal operation.
    fn require_sftp(
        &self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn SftpBackend>> {
        match self.sftp() {
            Some(sftp) => Some(sftp.clone()),
            None => {
                log::warn!("SftpPanel::{action}: no active SFTP backend");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "No active SFTP connection is available.",
                        cx,
                    ),
                    cx,
                );
                None
            }
        }
    }

    /// Rename selected entry.
    /// Opens a dialog with an InputState pre-filled with the current name → sftp.rename().
    pub(crate) fn do_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_rename: no selection");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Select a file or folder to rename.",
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };

        log::info!("SftpPanel::do_rename: \"{}\"", entry.name);

        let Some(sftp) = self.require_sftp("do_rename", window, cx) else {
            return;
        };
        let panel = cx.entity();
        let from_path = entry.path.clone();

        let name_state = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("New name");
            st.set_value(&entry.name, window, cx);
            st
        });

        let submit = {
            let name_state = name_state.clone();
            move |window: &mut Window, cx: &mut App| {
                let new_name = match validate_entry_name(&name_state.read(cx).value()) {
                    Ok(name) => name.to_string(),
                    Err(error) => {
                        window.push_notification(
                            notify(NotificationType::Warning, error.to_string(), cx),
                            cx,
                        );
                        return false;
                    }
                };
                let to_path = rename_target(&from_path, &new_name);
                log::info!("SftpPanel: rename \"{from_path}\" → \"{to_path}\"");
                let sftp = sftp.clone();
                let from = from_path.clone();
                run_mutation(
                    panel.clone(),
                    "rename",
                    format!("Renamed to \"{new_name}\"."),
                    "Rename failed",
                    async move { sftp.rename(from, to_path).await },
                    window,
                    cx,
                );
                // The dialog closes itself once the backend confirms.
                false
            }
        };

        FormDialog::new(
            "Rename",
            move |content, _window, cx| {
                content.child(labelled_field(
                    "New name",
                    FieldRequirement::Required,
                    Input::new(&name_state),
                    cx,
                ))
            },
            submit,
        )
        .confirm_label("Rename")
        .open(window, cx);
    }

    /// Delete selected entry (file or folder).
    /// Opens a confirm alert dialog → `sftp.remove()` for a file, or
    /// `sftp.remove_dir_all()` for a folder (recursive — the dialog says so).
    pub(crate) fn do_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_delete: no selection");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Select a file or folder to delete.",
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };

        log::info!(
            "SftpPanel::do_delete: \"{}\" (is_dir={})",
            entry.name,
            entry.is_dir
        );

        let Some(sftp) = self.require_sftp("do_delete", window, cx) else {
            return;
        };
        let panel = cx.entity();
        let path = entry.path.clone();
        let is_dir = entry.is_dir;
        let entry_name = entry.name.clone();
        let desc = delete_confirmation(&entry_name, is_dir);

        window.open_alert_dialog(cx, move |alert, _window, _cx| {
            alert
                .confirm()
                .title("Confirm Delete")
                .description(desc.clone())
                .footer({
                    let sftp = sftp.clone();
                    let path = path.clone();
                    let panel = panel.clone();
                    DialogFooter::new()
                        .child(Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ))
                        .child(Button::new("delete").label("Delete").danger().on_click(
                            move |_, window, cx| {
                                let operation_path = path.clone();
                                let operation_sftp = sftp.clone();
                                let operation_panel = panel.clone();
                                window
                                    .spawn(cx, async move |cx| {
                                        let result = if is_dir {
                                            operation_sftp.remove_dir_all(operation_path).await
                                        } else {
                                            operation_sftp.remove(operation_path).await
                                        };
                                        // The dialog may close before the background result arrives.
                                        _ = cx.update(|window, cx| match result {
                                            Ok(()) => {
                                                log::info!("SftpPanel: delete OK");
                                                window.push_notification(
                                                    notify(
                                                        NotificationType::Success,
                                                        "Deleted successfully.",
                                                        cx,
                                                    ),
                                                    cx,
                                                );
                                                operation_panel
                                                    .update(cx, |this, cx| this.refresh(cx));
                                                window.close_dialog(cx);
                                            }
                                            Err(e) => {
                                                log::error!("SftpPanel: delete failed: {e}");
                                                window.push_notification(
                                                    notify(
                                                        NotificationType::Error,
                                                        describe_failure("Delete failed", &e),
                                                        cx,
                                                    ),
                                                    cx,
                                                );
                                            }
                                        });
                                    })
                                    .detach();
                            },
                        ))
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(|_, _, _| false),
                )
        });
    }

    /// Create a new folder in the cwd.
    /// Opens a dialog with an InputState → sftp.mkdir().
    pub(crate) fn do_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("SftpPanel::do_new_folder: cwd=\"{}\"", self.browser().cwd());

        let Some(sftp) = self.require_sftp("do_new_folder", window, cx) else {
            return;
        };
        let panel = cx.entity();
        let cwd = self.browser().cwd().clone();

        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));

        let submit = {
            let name_state = name_state.clone();
            move |window: &mut Window, cx: &mut App| {
                let name = match validate_entry_name(&name_state.read(cx).value()) {
                    Ok(name) => name.to_string(),
                    Err(error) => {
                        window.push_notification(
                            notify(NotificationType::Warning, error.to_string(), cx),
                            cx,
                        );
                        return false;
                    }
                };
                let path = cwd.join(&name);
                log::info!("SftpPanel: mkdir \"{path}\"");
                let sftp = sftp.clone();
                run_mutation(
                    panel.clone(),
                    "mkdir",
                    format!("Folder \"{name}\" created."),
                    "Create folder failed",
                    async move { sftp.mkdir(path).await },
                    window,
                    cx,
                );
                // The dialog closes itself once the backend confirms.
                false
            }
        };

        FormDialog::new(
            "New Folder",
            move |content, _window, cx| {
                content.child(labelled_field(
                    "Folder name",
                    FieldRequirement::Required,
                    Input::new(&name_state),
                    cx,
                ))
            },
            submit,
        )
        .confirm_label("Create")
        .open(window, cx);
    }

    /// Show properties dialog — sftp.stat() → display detailed metadata.
    pub(crate) fn do_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entry = match self.selected_entry(cx) {
            Some(e) => e.clone(),
            None => {
                log::warn!("SftpPanel::do_properties: no selection");
                window.push_notification(
                    notify(
                        NotificationType::Warning,
                        "Select a file or folder to view properties.",
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };

        log::info!("SftpPanel::do_properties: \"{}\"", entry.name);

        let Some(sftp) = self.require_sftp("do_properties", window, cx) else {
            return;
        };
        let path = entry.path.clone();
        window
            .spawn(cx, async move |cx| {
                let result = sftp.stat(path).await;
                // The window may close before the background result arrives.
                _ = cx.update(|window, cx| match result {
                    Ok(stat) => open_properties_dialog(stat, window, cx),
                    Err(error) => {
                        log::error!("SftpPanel: stat failed: {error}");
                        window.push_notification(
                            notify(
                                NotificationType::Error,
                                describe_failure("Failed to get properties", &error),
                                cx,
                            ),
                            cx,
                        );
                    }
                });
            })
            .detach();
    }
}

/// Wording of the delete confirmation. A folder delete is recursive, so the
/// text must say that its contents go with it.
fn delete_confirmation(entry_name: &str, is_dir: bool) -> String {
    if is_dir {
        format!(
            "Are you sure you want to delete folder \"{entry_name}\" and all of its contents? This cannot be undone."
        )
    } else {
        format!("Are you sure you want to delete file \"{entry_name}\"?")
    }
}

/// User-facing text for a failed operation: SFTP status codes get a corrective
/// hint (permission denied vs. missing path), everything else shows the error.
fn describe_failure(what: &str, error: &AppError) -> String {
    match error.sftp_status() {
        Some(SftpStatus::PermissionDenied) => {
            format!("{what}: permission denied by the server.")
        }
        Some(SftpStatus::NoSuchFile) => {
            format!("{what}: the path no longer exists on the server. Refresh and try again.")
        }
        Some(SftpStatus::Failure) => {
            format!(
                "{what}: the server refused ({error}). The name may be in use or the folder not empty."
            )
        }
        _ => format!("{what}: {error}"),
    }
}

/// Run one backend mutation from a form dialog: on success notify, refresh the
/// listing and close the dialog; on failure notify and keep the dialog open.
fn run_mutation(
    panel: Entity<SftpPanel>,
    operation: &'static str,
    success_message: String,
    failure_prefix: &'static str,
    mutation: impl Future<Output = oneterm_core::Result<()>> + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    window
        .spawn(cx, async move |cx| {
            let result = mutation.await;
            // The dialog may close before the background result arrives.
            _ = cx.update(|window, cx| match result {
                Ok(()) => {
                    log::info!("SftpPanel: {operation} OK");
                    window.push_notification(
                        notify(NotificationType::Success, success_message, cx),
                        cx,
                    );
                    panel.update(cx, |this, cx| this.refresh(cx));
                    window.close_dialog(cx);
                }
                Err(error) => {
                    log::error!("SftpPanel: {operation} failed: {error}");
                    window.push_notification(
                        notify(
                            NotificationType::Error,
                            describe_failure(failure_prefix, &error),
                            cx,
                        ),
                        cx,
                    );
                }
            });
        })
        .detach();
}

fn open_properties_dialog(stat: FileEntry, window: &mut Window, cx: &mut App) {
    log::debug!(
        "SftpPanel: stat OK — size={}, perm={:#o}, uid={:?}, gid={:?}",
        stat.size,
        stat.permissions,
        stat.uid,
        stat.gid
    );

    let kind_str = if stat.is_dir { "Folder" } else { "File" };
    let size_text = Rc::new(if stat.is_dir {
        "-".to_string()
    } else {
        format!("{} ({} bytes)", format_size(stat.size), stat.size)
    });
    let modified_text = Rc::new(format_date(stat.modified));
    let accessed_text = Rc::new(format_date(stat.accessed));
    let perm_text = Rc::new(format_permissions(stat.permissions));
    let owner_text = Rc::new(format_owner(stat.owner.as_deref(), stat.uid));
    let group_text = Rc::new(format_owner(stat.group.as_deref(), stat.gid));
    let path_text = Rc::new(stat.path.to_string());
    let name_text = Rc::new(stat.name.clone());
    let is_symlink = stat.is_symlink;

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let name_text = name_text.clone();
        let size_text = size_text.clone();
        let modified_text = modified_text.clone();
        let accessed_text = accessed_text.clone();
        let perm_text = perm_text.clone();
        let owner_text = owner_text.clone();
        let group_text = group_text.clone();
        let path_text = path_text.clone();

        dialog
            .title("Properties")
            .w(px(480.))
            .content(move |content, _window, cx| {
                let theme = cx.theme();
                let label_w = px(100.0);
                let muted = theme.muted_foreground;

                let row = |label: &str, value: String| {
                    h_flex()
                        .w_full()
                        .gap_2()
                        .py_1()
                        .child(
                            div()
                                .w(label_w)
                                .flex_shrink_0()
                                .text_sm()
                                .text_color(muted)
                                .child(label.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .text_color(theme.foreground)
                                .truncate()
                                .child(value),
                        )
                };

                content.child(
                    v_flex()
                        .gap_0()
                        .w_full()
                        .child(row("Name:", (*name_text).clone()))
                        .child(row(
                            "Type:",
                            format!("{kind_str}{}", if is_symlink { " (symlink)" } else { "" }),
                        ))
                        .child(row("Size:", (*size_text).clone()))
                        .child(row("Modified:", (*modified_text).clone()))
                        .child(row("Accessed:", (*accessed_text).clone()))
                        .child(row("Permissions:", (*perm_text).clone()))
                        .child(row("Owner:", (*owner_text).clone()))
                        .child(row("Group:", (*group_text).clone()))
                        .child(row("Path:", (*path_text).clone())),
                )
            })
            .footer({
                DialogFooter::new().child(Button::new("close").label("Close").primary().on_click(
                    |_, window, cx| {
                        window.close_dialog(cx);
                    },
                ))
            })
            .button_props(
                DialogButtonProps::default()
                    .on_cancel(|_, _, _| true)
                    .on_ok(|_, window, cx| {
                        window.close_dialog(cx);
                        true
                    }),
            )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ARCH-12: rename and new-folder targets are POSIX paths on every host OS.
    #[test]
    fn rename_and_mkdir_targets_use_forward_slashes() {
        let entry = RemotePath::new("/home/u/a.txt");
        assert_eq!(rename_target(&entry, "b.txt").as_str(), "/home/u/b.txt");
        assert_eq!(
            rename_target(&RemotePath::new("/top"), "renamed").as_str(),
            "/renamed"
        );

        let cwd = RemotePath::new("/home/u");
        assert_eq!(cwd.join("new folder").as_str(), "/home/u/new folder");
        assert_eq!(RemotePath::root().join("dir").as_str(), "/dir");
    }

    #[test]
    fn failure_text_distinguishes_permission_denied_from_missing_paths() {
        let denied = AppError::Sftp {
            status: SftpStatus::PermissionDenied,
            message: "Permission denied".into(),
        };
        assert!(describe_failure("Rename failed", &denied).contains("permission denied"));
        let missing = AppError::Sftp {
            status: SftpStatus::NoSuchFile,
            message: String::new(),
        };
        assert!(describe_failure("Delete failed", &missing).contains("no longer exists"));
        assert_eq!(
            describe_failure("Rename failed", &AppError::msg("boom")),
            "Rename failed: boom"
        );
    }

    /// CORR-53: rename / new-folder names are single path components.
    #[test]
    fn entry_names_reject_slashes_and_dot_components() {
        assert_eq!(validate_entry_name("  notes.txt "), Ok("notes.txt"));
        assert_eq!(validate_entry_name("   "), Err(EntryNameError::Empty));
        assert_eq!(
            validate_entry_name("dir/file"),
            Err(EntryNameError::ContainsSlash)
        );
        assert_eq!(validate_entry_name("."), Err(EntryNameError::DotOrDotDot));
        assert_eq!(validate_entry_name(".."), Err(EntryNameError::DotOrDotDot));
        // A hidden file is a legitimate name.
        assert_eq!(validate_entry_name(".env"), Ok(".env"));
        assert!(EntryNameError::ContainsSlash.to_string().contains('/'));
    }

    #[test]
    fn folder_delete_confirmation_mentions_the_contents() {
        let text = delete_confirmation("logs", true);
        assert!(text.contains("\"logs\""));
        assert!(text.contains("all of its contents"));
        assert!(!delete_confirmation("a.txt", false).contains("contents"));
    }
}
