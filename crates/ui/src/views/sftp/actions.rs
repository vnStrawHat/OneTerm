//! File operations for the SFTP browser — rename, delete, new folder, properties.
//!
//! Split out from `file_browser.rs` to keep the file shorter.
//! These methods open a dialog (input/confirm) and then call the SFTP backend.

use std::path::Path;
use std::rc::Rc;

use gpui::{App, AppContext, ClickEvent, Context, ParentElement, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogButtonProps, DialogFooter},
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    v_flex,
};

use oneterm_core::FileStat;

use super::panel::SftpPanel;
use super::types::{format_date, format_owner, format_permissions, format_size};
use crate::notif_ext::notify;

impl SftpPanel {
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

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let from_path = entry.path.clone();

        let name_state = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("New name");
            st.set_value(&entry.name, window, cx);
            st
        });

        let name_ok = name_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let name_ok = name_ok.clone();
            let sftp = sftp.clone();
            let from_path = from_path.clone();
            let panel = panel.clone();
            move |_, window, cx| {
                let new_name = name_ok.read(cx).value().trim().to_string();
                if new_name.is_empty() {
                    window.push_notification(
                        notify(NotificationType::Warning, "Name cannot be empty.", cx),
                        cx,
                    );
                    return false;
                }

                // Build new path: parent + new_name
                let parent = from_path.parent().unwrap_or_else(|| Path::new("/"));
                let to_path = parent.join(&new_name);

                log::info!(
                    "SftpPanel: rename \"{}\" → \"{}\"",
                    from_path.display(),
                    to_path.display()
                );

                match sftp.rename(from_path.clone(), to_path) {
                    Ok(()) => {
                        log::info!("SftpPanel: rename OK");
                        window.push_notification(
                            notify(
                                NotificationType::Success,
                                format!("Renamed to \"{new_name}\"."),
                                cx,
                            ),
                            cx,
                        );
                        panel.update(cx, |this, cx| this.refresh(cx));
                        true
                    }
                    Err(e) => {
                        log::error!("SftpPanel: rename failed: {e}");
                        window.push_notification(
                            notify(NotificationType::Error, format!("Rename failed: {e}"), cx),
                            cx,
                        );
                        false
                    }
                }
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("Rename")
                .w(px(440.))
                .content({
                    let name_state = name_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child("New name"),
                                )
                                .child(Input::new(&name_state)),
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ))
                        .child(Button::new("save").label("Rename").primary().on_click(
                            move |_, window, cx| {
                                if save_for_click(&ClickEvent::default(), window, cx) {
                                    window.close_dialog(cx);
                                }
                            },
                        ))
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| {
                            save_for_kb(&ClickEvent::default(), window, cx)
                        }),
                )
        });
    }

    /// Delete selected entry (file or folder).
    /// Opens a confirm alert dialog → sftp.remove() or sftp.rmdir().
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

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let path = entry.path.clone();
        let is_dir = entry.is_dir;
        let entry_name = entry.name.clone();
        let kind_str = if is_dir { "folder" } else { "file" };
        let desc = format!("Are you sure you want to delete {kind_str} \"{entry_name}\"?");

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
                                log::info!("SftpPanel: deleting \"{}\"", path.display());
                                let result = if is_dir {
                                    sftp.rmdir(path.clone())
                                } else {
                                    sftp.remove(path.clone())
                                };
                                match result {
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
                                        panel.update(cx, |this, cx| this.refresh(cx));
                                        window.close_dialog(cx);
                                    }
                                    Err(e) => {
                                        log::error!("SftpPanel: delete failed: {e}");
                                        window.push_notification(
                                            notify(
                                                NotificationType::Error,
                                                format!("Delete failed: {e}"),
                                                cx,
                                            ),
                                            cx,
                                        );
                                    }
                                }
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
        log::info!("SftpPanel::do_new_folder: cwd=\"{}\"", self.cwd.display());

        let sftp = self.sftp.clone().unwrap();
        let panel = cx.entity();
        let cwd = self.cwd.clone();

        let name_state = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));

        let name_ok = name_state.clone();

        let save_logic: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool> = Rc::new({
            let name_ok = name_ok.clone();
            let sftp = sftp.clone();
            let cwd = cwd.clone();
            let panel = panel.clone();
            move |_, window, cx| {
                let name = name_ok.read(cx).value().trim().to_string();
                if name.is_empty() {
                    window.push_notification(
                        notify(
                            NotificationType::Warning,
                            "Folder name cannot be empty.",
                            cx,
                        ),
                        cx,
                    );
                    return false;
                }
                let path = cwd.join(&name);
                log::info!("SftpPanel: mkdir \"{}\"", path.display());
                match sftp.mkdir(path) {
                    Ok(()) => {
                        log::info!("SftpPanel: mkdir OK");
                        window.push_notification(
                            notify(
                                NotificationType::Success,
                                format!("Folder \"{name}\" created."),
                                cx,
                            ),
                            cx,
                        );
                        panel.update(cx, |this, cx| this.refresh(cx));
                        true
                    }
                    Err(e) => {
                        log::error!("SftpPanel: mkdir failed: {e}");
                        window.push_notification(
                            notify(
                                NotificationType::Error,
                                format!("Create folder failed: {e}"),
                                cx,
                            ),
                            cx,
                        );
                        false
                    }
                }
            }
        });

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let save_for_click = save_logic.clone();
            let save_for_kb = save_logic.clone();
            dialog
                .title("New Folder")
                .w(px(440.))
                .content({
                    let name_state = name_state.clone();
                    move |content, _window, cx| {
                        content.child(
                            v_flex()
                                .gap_1()
                                .w_full()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child("Folder name"),
                                )
                                .child(Input::new(&name_state)),
                        )
                    }
                })
                .footer({
                    DialogFooter::new()
                        .child(Button::new("cancel").label("Cancel").outline().on_click(
                            |_, window, cx| {
                                window.close_dialog(cx);
                            },
                        ))
                        .child(Button::new("create").label("Create").primary().on_click(
                            move |_, window, cx| {
                                if save_for_click(&ClickEvent::default(), window, cx) {
                                    window.close_dialog(cx);
                                }
                            },
                        ))
                })
                .button_props(
                    DialogButtonProps::default()
                        .on_cancel(|_, _, _| true)
                        .on_ok(move |_, window, cx| {
                            save_for_kb(&ClickEvent::default(), window, cx)
                        }),
                )
        });
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

        let sftp = self.sftp.clone().unwrap();
        let stat: FileStat = match sftp.stat(entry.path.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("SftpPanel: stat failed: {e}");
                window.push_notification(
                    notify(
                        NotificationType::Error,
                        format!("Failed to get properties: {e}"),
                        cx,
                    ),
                    cx,
                );
                return;
            }
        };

        log::debug!(
            "SftpPanel: stat OK — size={}, perm={:#o}, uid={:?}, gid={:?}",
            stat.size,
            stat.permissions,
            stat.uid,
            stat.gid
        );

        // Build detail rows — wrap in Rc for sharing across Fn closures.
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
        let path_text = Rc::new(stat.path.display().to_string());
        let name_text = Rc::new(stat.name.clone());
        let is_symlink = stat.is_symlink;

        window.open_dialog(cx, move |dialog, _window, _cx| {
            // Clone Rc values here so content closure can capture them by move.
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

                    // Helper: label + value row.
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
                    DialogFooter::new().child(
                        Button::new("close")
                            .label("Close")
                            .primary()
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
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
}
