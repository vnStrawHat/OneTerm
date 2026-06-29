//! `impl Render for SftpPanel` + render helpers cho layout chính
//! (breadcrumb, toolbar, transfer queue, file list).
//!
//! File list render bằng `gpui_component::table::DataTable` — thay thế
//! render thủ công header + rows trong `render_list.rs` (đã xoá).

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement as _,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    table::DataTable,
    v_flex,
};

use super::panel::SftpPanel;
use super::types::{PendingAction, SortColumn};

impl Render for SftpPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Execute pending action from context menu.
        if let Some(action) = self.pending_action.take() {
            log::debug!("SftpPanel: executing pending action: {action:?}");
            match action {
                PendingAction::Open(idx) => self.navigate_into(idx, cx),
                PendingAction::Download => self.do_download(_window, cx),
                PendingAction::Rename => self.do_rename(_window, cx),
                PendingAction::Delete => self.do_delete(_window, cx),
                PendingAction::Properties => self.do_properties(_window, cx),
                PendingAction::Upload => self.do_upload(_window, cx),
                PendingAction::NewFolder => self.do_new_folder(_window, cx),
                PendingAction::Refresh => self.refresh(cx),
            }
        }

        if self.sftp.is_none() {
            return self.render_no_connection(cx).into_any_element();
        }

        let theme = cx.theme();

        v_flex()
            .id("sftp-panel")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .child(self.render_breadcrumb(cx))
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

    /// Render breadcrumb: path + ↑ parent + ⟳ refresh.
    fn render_breadcrumb(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let hover_bg = muted.opacity(0.1);

        h_flex()
            .w_full()
            .h_8()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("sftp-parent-btn")
                    .cursor_pointer()
                    .px_1()
                    .py_0()
                    .rounded(px(3.))
                    .hover(move |t| t.bg(hover_bg))
                    .child(Icon::new(IconName::ArrowUp).xsmall().text_color(muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigate_parent(cx);
                    })),
            )
            .child(
                div()
                    .id("sftp-refresh-btn")
                    .cursor_pointer()
                    .px_1()
                    .py_0()
                    .rounded(px(3.))
                    .hover(move |t| t.bg(hover_bg))
                    .child(Icon::new(IconName::Redo).xsmall().text_color(muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.refresh(cx);
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(muted)
                    .child(self.cwd.display().to_string()),
            )
    }

    /// Render toolbar: New Folder, Upload, Download, Rename, Delete, Properties,
    /// Columns (dropdown config ẩn/hiện cột), + selection info.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger_fg = theme.danger_foreground;

        // Build Columns dropdown — checkbox cho mỗi cột (Name luôn checked + disabled).
        let panel = cx.entity();
        let col_configs = self
            .table
            .read(cx)
            .delegate()
            .col_configs
            .iter()
            .map(|c| (c.col, c.label.to_string(), c.visible))
            .collect::<Vec<_>>();

        let columns_btn = Button::new("sftp-columns")
            .label("Columns")
            .icon(Icon::new(IconName::Settings2).xsmall())
            .small()
            .ghost()
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
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
            .border_b_1()
            .border_color(theme.border)
            // New Folder
            .child(
                Button::new("sftp-new-folder")
                    .label("New Folder")
                    .icon(Icon::new(IconName::Plus).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_new_folder(window, cx);
                    })),
            )
            // Upload
            .child(
                Button::new("sftp-upload")
                    .label("Upload")
                    .icon(Icon::new(IconName::ArrowUp).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_upload(window, cx);
                    })),
            )
            // Download
            .child(
                Button::new("sftp-download")
                    .label("Download")
                    .icon(Icon::new(IconName::ArrowDown).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_download(window, cx);
                    })),
            )
            // Rename
            .child(
                Button::new("sftp-rename")
                    .label("Rename")
                    .icon(Icon::new(IconName::Replace).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_rename(window, cx);
                    })),
            )
            // Delete (danger)
            .child(
                Button::new("sftp-delete")
                    .label("Delete")
                    .icon(Icon::new(IconName::Delete).xsmall())
                    .small()
                    .ghost()
                    .text_color(danger_fg)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_delete(window, cx);
                    })),
            )
            // Properties
            .child(
                Button::new("sftp-properties")
                    .label("Properties")
                    .icon(Icon::new(IconName::Info).xsmall())
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.do_properties(window, cx);
                    })),
            )
            // Columns config dropdown
            .child(columns_btn)
            // Spacer
            .child(div().flex_1())
            // Selection info
            .child(
                div().text_xs().text_color(muted).child(
                    self.selected
                        .map(|ix| {
                            self.table
                                .read(cx)
                                .delegate()
                                .entries
                                .get(ix)
                                .map(|e| format!("{} selected", e.name))
                                .unwrap_or_else(|| "? selected".to_string())
                        })
                        .unwrap_or_else(|| "No selection".to_string()),
                ),
            )
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