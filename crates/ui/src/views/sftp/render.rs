//! `impl Render for SftpPanel` + render helpers cho layout chính
//! (breadcrumb, toolbar, column headers, file list).
//!
//! Tách từ `file_browser.rs` để giảm độ dài file.

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement, Render,
    StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::{ContextMenuExt as _, PopupMenuItem},
    v_flex,
};

use super::panel::SftpPanel;
use super::types::{COLUMNS, PendingAction};

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

    /// Render toolbar: Upload, Download, Rename, Delete, New Folder buttons.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger_fg = theme.danger_foreground;

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
            // Spacer
            .child(div().flex_1())
            // Selection info
            .child(
                div().text_xs().text_color(muted).child(
                    self.selected
                        .map(|ix| {
                            self.entries
                                .get(ix)
                                .map(|e| format!("{} selected", e.name))
                                .unwrap_or_else(|| "? selected".to_string())
                        })
                        .unwrap_or_else(|| "No selection".to_string()),
                ),
            )
    }

    /// Render column headers — clickable để sort, có sort indicator.
    fn render_column_headers(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let accent = theme.accent;
        let header_bg = theme.muted.opacity(0.3);
        let hover_bg = theme.muted.opacity(0.5);

        let mut header = h_flex()
            .w_full()
            .h_6()
            .flex_shrink_0()
            .items_center()
            .px_2()
            .gap_2()
            .bg(header_bg)
            .border_b_1()
            .border_color(theme.border);

        for (i, col_def) in COLUMNS.iter().enumerate() {
            let is_active_sort = self.sort_col == col_def.col;
            let sort_dir = self.sort_dir;

            // Sort indicator icon.
            let sort_icon = if is_active_sort {
                match sort_dir {
                    super::types::SortDir::Asc => {
                        Some(Icon::new(IconName::ChevronUp).xsmall().text_color(accent))
                    }
                    super::types::SortDir::Desc => {
                        Some(Icon::new(IconName::ChevronDown).xsmall().text_color(accent))
                    }
                }
            } else {
                None
            };

            // Text color: accent if active sort, muted otherwise.
            let text_color = if is_active_sort { accent } else { muted };

            // Build cell.
            let label = col_def.label.to_string();
            let col = col_def.col;
            let right_align = col_def.right_align;

            let mut cell = div()
                .id(gpui::ElementId::NamedInteger("sftp-col".into(), i as u64))
                .h_full()
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .rounded(px(2.))
                .hover(move |t| t.bg(hover_bg))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sort_by(col, cx);
                }));

            if right_align {
                cell = cell.justify_end();
            }

            // Fixed width or flex.
            if let Some(w) = col_def.width {
                cell = cell.w(px(w)).flex_shrink_0();
            } else {
                cell = cell.flex_1().min_w_0();
            }

            // Label text.
            cell = cell.child(div().text_xs().text_color(text_color).child(label));

            // Sort indicator.
            if let Some(icon) = sort_icon {
                cell = cell.child(icon);
            }

            header = header.child(cell);
        }

        header.into_any_element()
    }

    /// Render file list — column headers + scrollable rows.
    fn render_file_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        if self.loading {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Loading...")
                .into_any_element();
        }

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

        if self.entries.is_empty() {
            let panel = cx.entity();
            return div()
                .id("sftp-empty-area")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Empty directory.")
                .context_menu({
                    let panel = panel.clone();
                    move |menu, _window: &mut Window, _cx| {
                        log::debug!("SftpPanel: context menu for empty area");
                        menu.item(PopupMenuItem::new("Upload").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Upload);
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("New Folder").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::NewFolder);
                                    cx.notify();
                                });
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Refresh").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Refresh);
                                    cx.notify();
                                });
                            }
                        }))
                    }
                })
                .into_any_element();
        }

        // Column headers (fixed, not scrollable).
        let headers = self.render_column_headers(cx);

        // Scrollable file list.
        let mut list = v_flex()
            .id("sftp-file-list")
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();

        for (idx, entry) in self.entries.iter().enumerate() {
            list = list.child(self.render_entry_row(idx, entry, cx));
        }

        v_flex()
            .flex_1()
            .min_h_0()
            .child(headers)
            .child(list)
            .into_any_element()
    }
}
