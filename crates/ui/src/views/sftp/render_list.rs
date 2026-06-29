//! Render 1 row trong file list — icon, columns, click + context menu.
//!
//! Tách từ `file_browser.rs` để giảm độ dài file.

use gpui::{
    ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, Window, div, hsla,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    menu::{ContextMenuExt as _, PopupMenuItem},
};

use myterm2_core::FileEntry;

use super::panel::SftpPanel;
use super::types::{PendingAction, format_date, format_owner, format_permissions, format_size};

impl SftpPanel {
    /// Render 1 row trong file list.
    pub(crate) fn render_entry_row(
        &self,
        idx: usize,
        entry: &FileEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let is_selected = self.selected == Some(idx);

        let icon = if entry.is_dir {
            Icon::new(IconName::Folder)
                .xsmall()
                .text_color(theme.foreground)
        } else {
            Icon::new(IconName::File)
                .xsmall()
                .text_color(theme.muted_foreground)
        };

        let selected_bg = theme.accent.opacity(0.1);
        let row_bg = if is_selected {
            selected_bg
        } else {
            hsla(0.0, 0.0, 0.0, 0.0)
        };

        let name_color = if entry.is_dir {
            theme.foreground
        } else {
            theme.muted_foreground
        };

        let name_text = entry.name.clone();
        let name_for_log = name_text.clone();

        // Format cell values.
        let date_text = format_date(entry.modified);
        let size_text = if entry.is_dir {
            String::new()
        } else {
            format_size(entry.size)
        };
        let perm_text = format_permissions(entry.permissions);
        let owner_text = format_owner(entry.owner.as_deref(), entry.uid);
        let group_text = format_owner(entry.group.as_deref(), entry.gid);

        // Cell text color: muted for all columns except name.
        let muted = theme.muted_foreground;

        gpui_component::v_flex()
            .id(gpui::ElementId::NamedInteger(
                "sftp-entry".into(),
                idx as u64,
            ))
            .w_full()
            .h_7()
            .flex_shrink_0()
            .cursor_pointer()
            .bg(row_bg)
            .hover(move |t| t.bg(theme.accent.opacity(0.05)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                let click_count = event.click_count();
                log::debug!(
                    "SftpPanel: click entry {idx} (count={click_count}) — \"{name}\"",
                    name = name_for_log
                );
                this.selected = Some(idx);
                if click_count >= 2 {
                    log::debug!("SftpPanel: double-click → navigate_into({idx})");
                    this.navigate_into(idx, cx);
                }
                cx.notify();
            }))
            // Context menu — right-click trên entry row.
            .context_menu({
                let panel = cx.entity();
                let is_dir = entry.is_dir;
                move |menu, _window: &mut Window, cx| {
                    // Select entry on right-click.
                    panel.update(cx, |this, cx| {
                        this.selected = Some(idx);
                        cx.notify();
                    });

                    log::debug!("SftpPanel: context menu for entry {idx} (is_dir={is_dir})");

                    // Build menu — first item depends on type.
                    let menu = if is_dir {
                        menu.item(PopupMenuItem::new("Open").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Open(idx));
                                    cx.notify();
                                });
                            }
                        }))
                    } else {
                        menu.item(PopupMenuItem::new("Download").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Download);
                                    cx.notify();
                                });
                            }
                        }))
                    };

                    menu.separator()
                        .item(PopupMenuItem::new("Rename").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Rename);
                                    cx.notify();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Delete").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Delete);
                                    cx.notify();
                                });
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Properties").on_click({
                            let panel = panel.clone();
                            move |_, _window, cx| {
                                panel.update(cx, |this, cx| {
                                    this.pending_action = Some(PendingAction::Properties);
                                    cx.notify();
                                });
                            }
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Upload").on_click({
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
            // Row content: h_flex with all columns matching header widths.
            .child(
                gpui_component::h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .gap_2()
                    .px_2()
                    // ── Name column (flex, truncated) ──
                    .child(
                        gpui_component::h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_1()
                            .child(div().w_4().flex_shrink_0().child(icon))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .text_color(name_color)
                                    .truncate() // overflow_hidden + nowrap + ellipsis
                                    .child(name_text.clone()),
                            ),
                    )
                    // ── Date Modified column (130px) ──
                    .child(
                        div()
                            .w(gpui::px(130.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(date_text),
                    )
                    // ── Size column (70px, right-aligned) ──
                    .child(
                        div()
                            .w(gpui::px(70.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .text_align(gpui::TextAlign::Right)
                            .child(size_text),
                    )
                    // ── Permissions column (140px) ──
                    .child(
                        div()
                            .w(gpui::px(140.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(perm_text),
                    )
                    // ── Owner column (80px) ──
                    .child(
                        div()
                            .w(gpui::px(80.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(owner_text),
                    )
                    // ── Group column (80px) ──
                    .child(
                        div()
                            .w(gpui::px(80.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(group_text),
                    ),
            )
    }
}
