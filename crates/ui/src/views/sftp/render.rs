//! `impl Render for SftpPanel` + render helpers cho layout chính
//! (toolbar, transfer queue, file list).
//!
//! File list render bằng `gpui_component::table::DataTable` — thay thế
//! render thủ công header + rows trong `render_list.rs` (đã xoá).

use gpui::{
    Context, Focusable as _, InteractiveElement as _, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::Input,
    menu::{DropdownMenu as _, PopupMenuItem},
    table::DataTable,
    v_flex,
};

use super::panel::SftpPanel;
use super::types::{PendingAction, SortColumn};

impl Render for SftpPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Execute pending action from context menu.
        if let Some(action) = self.pending_action.take() {
            log::debug!("SftpPanel: executing pending action: {action:?}");
            match action {
                PendingAction::Open(idx) => self.navigate_into(idx, cx),
                PendingAction::Download => self.do_download(window, cx),
                PendingAction::Rename => self.do_rename(window, cx),
                PendingAction::Delete => self.do_delete(window, cx),
                PendingAction::Properties => self.do_properties(window, cx),
                PendingAction::Upload => self.do_upload(window, cx),
                PendingAction::NewFolder => self.do_new_folder(window, cx),
                PendingAction::Refresh => self.refresh(cx),
            }
        }

        if self.sftp.is_none() {
            return self.render_no_connection(cx).into_any_element();
        }

        // Sync path input value với cwd (chỉ khi input không đang focus).
        let cwd_display = self.cwd.display().to_string();
        let path_focused = self.path_input.read(cx).focus_handle(cx).is_focused(window);
        let path_value = self.path_input.read(cx).value().to_string();
        if !path_focused && path_value != cwd_display {
            self.path_input.update(cx, |state, cx| {
                state.set_value(cwd_display, window, cx);
            });
        }

        let theme = cx.theme();

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .child(self.render_toolbar(cx))
            .child(self.render_transfer_queue(cx))
            .child(self.render_file_list(cx))
            .into_any_element()
    }
}

impl SftpPanel {
    /// Render khi không có SFTP connection.
    fn render_no_connection(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No SFTP connection.")
    }

    /// Render toolbar — path input (flex-1) + back, refresh, "..." (right-aligned).
    ///
    /// Path input hiển thị cwd, Enter → goto path (highlight lỗi nếu không tồn tại).
    /// "..." button mở popup menu: New Folder, Upload, Download, Rename, Delete,
    /// Properties, separator, Columns config (checkbox).
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        // Border color cho path input — red nếu error, else theme border.
        let path_border = if self.path_error {
            theme.danger
        } else {
            theme.border
        };

        // Build "..." menu — toolbar actions + Columns config.
        let panel = cx.entity();
        let col_configs = self
            .table
            .read(cx)
            .delegate()
            .col_configs
            .iter()
            .map(|c| (c.col, c.label.to_string(), c.visible))
            .collect::<Vec<_>>();

        let more_btn = Button::new("sftp-more")
            .icon(Icon::new(IconName::EllipsisVertical).small())
            .small()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu
                    .item(
                        PopupMenuItem::new("New Folder")
                            .icon(Icon::new(IconName::Plus))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::NewFolder);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Upload")
                            .icon(Icon::new(IconName::ArrowUp))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Upload);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Download")
                            .icon(Icon::new(IconName::ArrowDown))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Download);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Rename")
                            .icon(Icon::new(IconName::Replace))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Rename);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Delete")
                            .icon(Icon::new(IconName::Delete))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Delete);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("Properties")
                            .icon(Icon::new(IconName::Info))
                            .on_click({
                                let panel = panel.clone();
                                move |_, _, cx| {
                                    panel.update(cx, |this, cx| {
                                        this.pending_action = Some(PendingAction::Properties);
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .separator();

                // Columns config — checkbox cho mỗi cột (Name luôn checked + disabled).
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
                Input::new(&self.path_input)
                    // .appearance(false)
                    .border_b_1()
                    .border_color(path_border)
                    // .text_sm()
                    .small(),
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
            // Refresh button
            .child(
                Button::new("sftp-refresh")
                    .icon(Icon::new(IconName::Redo).small())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh(cx);
                    })),
            )
            // "..." button — popup menu với toolbar actions + Columns config.
            .child(more_btn)
    }

    /// Render file list — DataTable (hoặc error message).
    ///
    /// Loading + empty state được DataTable tự xử lý qua delegate
    /// (`loading()`, `render_empty`).
    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        if let Some(err) = &self.error {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .text_color(theme.danger_foreground)
                .child(format!("Error: {err}"))
                .into_any_element();
        }

        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                DataTable::new(&self.table)
                    .bordered(false)
                    .scrollbar_visible(true, true)
                    .small(),
            )
            .into_any_element()
    }
}
