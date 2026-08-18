//! `impl Render for SftpPanel` + render helpers for the main layout
//! (toolbar, transfer queue, file list).
//!
//! The file list is rendered with `gpui_component::table::DataTable`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, ExternalPaths, Focusable as _, InteractiveElement as _, IntoElement, ParentElement,
    Render, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::Input,
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::NotificationType,
    table::DataTable,
    v_flex,
};
use oneterm_actions::{
    SftpDelete, SftpDownload, SftpNewFolder, SftpOpen, SftpProperties, SftpRefresh, SftpRename,
    SftpUploadFiles, SftpUploadFolder,
};
use oneterm_theme::icon::AppIcon;
use oneterm_theme::notif_ext::notify;

use super::panel::SftpPanel;
use super::table_delegate_menu::on_click_panel;
use super::types::SortColumn;

impl Render for SftpPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.sftp().is_none() {
            return self.render_no_connection(cx).into_any_element();
        }

        // Sync the path input value with cwd (only when the input is not focused).
        // This is a projection of state into the input widget, not an action.
        let cwd_display = self.browser().cwd().to_string();
        let path_input = self.path_input().clone();
        let path_focused = path_input.read(cx).focus_handle(cx).is_focused(window);
        let path_value = path_input.read(cx).value().to_string();
        if !path_focused && path_value != cwd_display {
            path_input.update(cx, |state, cx| {
                state.set_value(cwd_display, window, cx);
            });
        }

        let theme = cx.theme();

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(self.panel_focus_handle())
            // SFTP context-menu action handlers — also fired by global key bindings.
            .on_action(cx.listener(|this, _: &SftpOpen, w, cx| {
                // Open = navigate into a directory, download a file.
                match (this.browser().selected(), this.selected_entry(cx)) {
                    (Some(ix), Some(entry)) if entry.is_dir => this.navigate_into(ix, cx),
                    (Some(_), Some(_)) => this.do_download(w, cx),
                    _ => {}
                }
            }))
            .on_action(cx.listener(|this, _: &SftpDownload, w, cx| this.do_download(w, cx)))
            .on_action(cx.listener(|this, _: &SftpRename, w, cx| this.do_rename(w, cx)))
            .on_action(cx.listener(|this, _: &SftpDelete, w, cx| this.do_delete(w, cx)))
            .on_action(cx.listener(|this, _: &SftpProperties, w, cx| this.do_properties(w, cx)))
            .on_action(cx.listener(|this, _: &SftpUploadFiles, w, cx| this.do_upload(false, w, cx)))
            .on_action(cx.listener(|this, _: &SftpUploadFolder, w, cx| this.do_upload(true, w, cx)))
            .on_action(cx.listener(|this, _: &SftpNewFolder, w, cx| this.do_new_folder(w, cx)))
            .on_action(cx.listener(|this, _: &SftpRefresh, _, cx| this.refresh(cx)))
            .bg(theme.background)
            .child(self.render_toolbar(window, cx))
            .child(self.render_file_list(cx))
            .child(self.render_transfer_queue(cx))
            .into_any_element()
    }
}

impl SftpPanel {
    /// Render when there is no SFTP connection.
    fn render_no_connection(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sftp-panel")
            .size_full()
            .track_focus(self.panel_focus_handle())
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No SFTP connection.")
    }

    /// Render toolbar — path input (flex-1) + back, refresh, "..." (right-aligned).
    ///
    /// The path input shows cwd; Enter → goto path (highlights an error if it doesn't exist).
    /// The "..." button opens a popup menu: New Folder, Upload, Download, Rename, Delete,
    /// Properties, separator, Columns config (checkbox).
    fn render_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        // Border color for the path input:
        // - Error → theme.danger (always, even when focused)
        // - Focused (no error) → do NOT override, so Input.focused_border(cx)
        //   sets border_color = theme.ring (focus highlight)
        // - Default → theme.border
        let path_focused = self
            .path_input()
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let path_error = self.browser().path_error();
        let show_custom_border = path_error || !path_focused;
        let path_border = if path_error {
            theme.danger
        } else {
            theme.border
        };

        // "Sync to terminal cwd" button state — read the terminal's live cwd.
        let terminal_cwd = self.follow().terminal_cwd();
        let sync_enabled = terminal_cwd.is_some();
        let sync_tooltip = match &terminal_cwd {
            Some(p) => format!("Go to terminal's current directory: {p}"),
            None => "Terminal has not reported a directory (needs shell integration / OSC 7)"
                .to_string(),
        };

        // Build "..." menu — toolbar actions + Columns config.
        let panel = cx.entity();
        let panel_weak = panel.downgrade();
        let col_configs = self
            .table()
            .read(cx)
            .delegate()
            .col_configs
            .iter()
            .map(|c| (c.col, c.label.to_string(), c.visible))
            .collect::<Vec<_>>();

        let follow_terminal_cwd = self.follow().enabled();

        let more_btn = Button::new("sftp-more")
            .icon(Icon::new(IconName::EllipsisVertical).small())
            .small()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu
                    .item(
                        PopupMenuItem::new("New Folder")
                            .icon(Icon::new(IconName::Plus))
                            .on_click(on_click_panel(panel_weak.clone(), SftpPanel::do_new_folder)),
                    )
                    .item(
                        PopupMenuItem::new("Upload Files")
                            .icon(Icon::new(IconName::ArrowUp))
                            .on_click(on_click_panel(panel_weak.clone(), |this, window, cx| {
                                this.do_upload(false, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new("Upload Folder")
                            .icon(Icon::new(IconName::ArrowUp))
                            .on_click(on_click_panel(panel_weak.clone(), |this, window, cx| {
                                this.do_upload(true, window, cx)
                            })),
                    )
                    .item(
                        PopupMenuItem::new("Download")
                            .icon(Icon::new(IconName::ArrowDown))
                            .on_click(on_click_panel(panel_weak.clone(), SftpPanel::do_download)),
                    )
                    .item(
                        PopupMenuItem::new("Rename")
                            .icon(Icon::new(IconName::Replace))
                            .on_click(on_click_panel(panel_weak.clone(), SftpPanel::do_rename)),
                    )
                    .item(
                        PopupMenuItem::new("Delete")
                            .icon(Icon::new(IconName::Delete))
                            .on_click(on_click_panel(panel_weak.clone(), SftpPanel::do_delete)),
                    )
                    .item(
                        PopupMenuItem::new("Properties")
                            .icon(Icon::new(IconName::Info))
                            .on_click(on_click_panel(panel_weak.clone(), SftpPanel::do_properties)),
                    )
                    .separator()
                    .item({
                        let panel = panel.clone();
                        PopupMenuItem::element(move |_, _cx| {
                            let panel = panel.clone();
                            // The popup menu renders an empty icon placeholder (12px) + gap (4px)
                            // to the left of every ElementItem when other menu items have icons.
                            // Negate that 16px offset so the Checkbox aligns flush left.
                            div().w_full().ml_neg_4().child(
                                Checkbox::new("sftp-follow-cwd")
                                    .small()
                                    .label("Follow Terminal Cwd")
                                    .checked(follow_terminal_cwd)
                                    .on_click(move |checked: &bool, _, cx| {
                                        panel.update(cx, |this, cx| {
                                            // Sync the flag to the checkbox's new state.
                                            if this.follow().enabled() != *checked {
                                                this.toggle_follow_terminal_cwd(cx);
                                            }
                                        });
                                    }),
                            )
                        })
                    })
                    .separator();

                // Columns config — a checkbox for each column (Name is always checked + disabled).
                for (col, label, visible) in &col_configs {
                    let is_name = *col == SortColumn::Name;
                    let item = PopupMenuItem::new(label.clone())
                        .checked(*visible)
                        .disabled(is_name);
                    let item = if !is_name {
                        let panel = panel.clone();
                        let col = *col;
                        item.on_click(move |_, _, cx| {
                            panel.update(cx, |this, cx| {
                                this.toggle_column(col, cx);
                            });
                        })
                    } else {
                        item
                    };
                    menu = menu.item(item);
                }

                menu
            });

        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .py_5()
            .border_b_1()
            .border_color(theme.border)
            // Path input — flex-1, border-bottom only, transparent bg.
            .child(
                Input::new(self.path_input())
                    .flex_1()
                    .border_b_1()
                    .border_t_0()
                    .border_l_0()
                    .border_r_0()
                    .when(show_custom_border, |input| input.border_color(path_border))
                    .small()
                    .bg(gpui::transparent_black()),
            )
            // Back button
            .child(
                Button::new("sftp-back")
                    .icon(Icon::new(IconName::ArrowLeft).small())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigate_parent(cx);
                    })),
            )
            // Sync-to-terminal-cwd button — jump SFTP to the SSH shell's cwd.
            .child(
                Button::new("sftp-sync-cwd")
                    .icon(Icon::new(AppIcon::FolderSync).small())
                    .small()
                    .ghost()
                    .disabled(!sync_enabled)
                    .tooltip(sync_tooltip)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sync_to_terminal_cwd(cx);
                    })),
            )
            // Refresh button
            .child(
                Button::new("sftp-refresh")
                    .icon(Icon::new(AppIcon::Refresh).small())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh(cx);
                    })),
            )
            // "..." button — popup menu with toolbar actions + Columns config.
            .child(more_btn)
    }

    /// Render file list — DataTable, with an error banner above it when the
    /// last listing failed (the previous entries stay visible).
    ///
    /// Loading + empty states are handled by DataTable itself via the delegate
    /// (`loading()`, `render_empty`).
    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let error_banner = self.browser().error().map(|err| {
            div()
                .id("sftp-listing-error")
                .w_full()
                .flex_shrink_0()
                .px_2()
                .py_1()
                .text_xs()
                .bg(theme.danger.opacity(0.12))
                .text_color(theme.danger)
                .child(format!(
                    "Could not open \"{}\": {err}",
                    self.browser().cwd()
                ))
        });

        v_flex()
            .id("sftp-file-list")
            .flex_1()
            .min_h_0()
            .children(error_banner)
            .child(
                DataTable::new(self.table())
                    .bordered(false)
                    .scrollbar_visible(true, true)
                    .small(),
            )
            // Drag & drop external files → upload to remote cwd.
            .can_drop(|drag, _window, _cx| drag.is::<ExternalPaths>())
            .on_drop(
                cx.listener(move |this, external_paths: &ExternalPaths, window, cx| {
                    let paths: Vec<_> = external_paths.paths().to_vec();
                    log::info!(
                        "SftpPanel: on_drop — {} external path(s) dropped",
                        paths.len()
                    );
                    if this.sftp().is_some() {
                        this.do_upload_paths(paths, cx);
                    } else {
                        log::warn!("SftpPanel: on_drop — no SFTP connection, ignoring");
                        window.push_notification(
                            notify(
                                NotificationType::Warning,
                                "No active SFTP connection — dropped files were not uploaded.",
                                cx,
                            ),
                            cx,
                        );
                    }
                }),
            )
            .into_any_element()
    }
}
