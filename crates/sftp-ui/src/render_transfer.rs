//! Render transfer queue — show progress for ongoing transfers.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement, StatefulInteractiveElement as _,
    Styled, div, px,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    progress::Progress,
    v_flex,
};

use super::panel::SftpPanel;
use super::types::{TransferDirection, TransferStatus};

impl SftpPanel {
    /// Render transfer queue — show progress for ongoing transfers.
    /// Only renders when self.transfers is not empty.
    pub(crate) fn render_transfer_queue(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.transfers().is_empty() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let danger = theme.danger;

        // Count active vs completed
        let active_count = self.transfers().active_count();
        let completed_count = self.transfers().items().len() - active_count;

        let mut queue = v_flex()
            .w_full()
            .flex_shrink_0()
            .max_h(px(200.0))
            .border_t_1()
            .border_color(theme.border);

        // Header: "Transfers" + count + Clear button
        queue = queue.child(
            h_flex()
                .w_full()
                .h_7()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .px_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child("Transfers"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("{active_count} active, {completed_count} done")),
                )
                .child(div().flex_1())
                .child(
                    Button::new("sftp-clear-transfers")
                        .label("Clear")
                        .xsmall()
                        .ghost()
                        .disabled(completed_count == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_completed_transfers(cx);
                        })),
                ),
        );

        // Transfer items
        let mut list = v_flex()
            .id("sftp-transfer-list")
            .w_full()
            .overflow_y_scroll();

        for item in self.transfers().items() {
            // Direction icon
            let dir_icon = match item.direction {
                TransferDirection::Upload => {
                    Icon::new(IconName::ArrowUp).small().text_color(theme.green)
                }
                TransferDirection::Download => Icon::new(IconName::ArrowDown)
                    .small()
                    .text_color(theme.cyan),
            };

            // Status indicator color
            let progress_color = match item.status {
                TransferStatus::InProgress => theme.foreground,
                TransferStatus::Completed => theme.success,
                TransferStatus::Cancelled => muted,
                TransferStatus::Error => danger,
            };

            list = list.child(
                h_flex()
                    .w_full()
                    .h_6()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .px_2()
                    // Direction icon
                    .child(div().w_4().flex_shrink_0().child(dir_icon))
                    // Filename
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .truncate()
                            .text_color(theme.foreground)
                            .child(item.filename.clone()),
                    )
                    // Progress bar
                    .child(
                        div().w(px(100.0)).flex_shrink_0().child(
                            Progress::new(gpui::ElementId::NamedInteger(
                                "sftp-transfer".into(),
                                item.id as u64,
                            ))
                            .xsmall()
                            .color(progress_color)
                            .value((item.progress * 100.0) as f32),
                        ),
                    )
                    // Percentage + status
                    .child(
                        div()
                            .w(px(60.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(theme.foreground)
                            .child(match item.status {
                                TransferStatus::InProgress => {
                                    format!("{:.0}%", item.progress * 100.0)
                                }
                                TransferStatus::Completed => "Done".to_string(),
                                TransferStatus::Cancelled => "Cancelled".to_string(),
                                TransferStatus::Error => "Error".to_string(),
                            }),
                    )
                    // Error message (if any)
                    .when(item.error.is_some(), |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(danger)
                                .truncate()
                                .child(item.error.clone().unwrap_or_default()),
                        )
                    })
                    // Cancel button — only shown when InProgress.
                    .when(item.status == TransferStatus::InProgress, |this| {
                        let cancel_id = item.id;
                        this.child(
                            div().flex_shrink_0().child(
                                Button::new(gpui::ElementId::NamedInteger(
                                    "sftp-cancel-transfer".into(),
                                    item.id as u64,
                                ))
                                .small()
                                .ghost()
                                .text_color(theme.foreground)
                                .icon(IconName::Close)
                                .tooltip("Cancel transfer")
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.cancel_transfer(cancel_id, cx);
                                    },
                                )),
                            ),
                        )
                    }),
            );
        }

        queue = queue.child(list);
        queue.into_any_element()
    }
}
