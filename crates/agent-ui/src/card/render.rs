use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TextAlign, div,
    prelude::FluentBuilder as _, px, relative,
};
use gpui_component::{Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_state::{AgentCard, FileEntry, Lifecycle, ModelInfo, ToolRun};

use crate::AgentListView;

use super::{
    Palette, action_icon, action_word, activity_chip, fmt_ago, fmt_tokens, lifecycle_summary,
    shorten_path, state_accent, state_indicator,
};

impl AgentListView {
    /// Render one compact agent card.
    pub(crate) fn render_card(
        &self,
        card: &AgentCard,
        pal: &Palette,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let accent = state_accent(card, pal);
        let terminal_key = card.terminal_key;
        let card_id = SharedString::from(format!(
            "agent-card-{:?}-{}",
            card.terminal_key, card.agent_id
        ));
        let dim =
            matches!(card.lifecycle, Lifecycle::Ended { .. }) || card.lifecycle == Lifecycle::Stale;

        let mut body = v_flex().w_full().gap_1().child(self.card_header(card, pal));

        if let Some(model) = &card.model {
            body = body.child(self.model_row(card, model, pal));
        }

        if let Some(row) = self.activity_row(card, pal) {
            body = body.child(row);
        }

        if let Some(file) = card.recent_files.back() {
            body = body.child(file_row(file, pal));
        }

        body = body.child(self.footer_row(card, pal));

        v_flex()
            .id(card_id)
            .w_full()
            .p_2()
            .gap_1()
            .border_l_2()
            .border_r_2()
            .border_color(accent)
            .bg(accent.opacity(0.05))
            .when(dim, |this| this.opacity(0.75))
            .cursor_pointer()
            .hover(|this| this.bg(pal.accent.opacity(0.12)))
            .on_click(cx.listener(move |_this, _ev, window, cx| {
                oneterm_state::agent_focus::focus_terminal(terminal_key, window, cx);
            }))
            .child(body)
            .into_any_element()
    }

    fn card_header(&self, card: &AgentCard, pal: &Palette) -> impl IntoElement {
        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(state_indicator(card, pal))
            .child(lifecycle_summary(card, pal))
    }

    fn model_row(&self, card: &AgentCard, model: &ModelInfo, pal: &Palette) -> impl IntoElement {
        let mut line = h_flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(pal.muted)
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(pal.foreground)
                    .child(model.provider.clone()),
            )
            .child(div().child(">"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(pal.foreground)
                    .child(model.display_name().to_string()),
            );
        if model.reasoning {
            line = line.child(
                div()
                    .px_1p5()
                    .rounded_sm()
                    .bg(pal.info.opacity(0.18))
                    .text_color(pal.info)
                    .text_xs()
                    .child("reasoning"),
            );
        }

        let bar = context_bar(card, model, pal);

        v_flex()
            .w_full()
            .gap_0p5()
            .child(line)
            .when_some(bar, |this, b| this.child(b))
    }

    fn activity_row(&self, card: &AgentCard, pal: &Palette) -> Option<AnyElement> {
        if let Some(run) = &card.current_tool {
            let detail = run
                .target
                .clone()
                .or_else(|| run.args.clone())
                .unwrap_or_default();
            let mut row = h_flex()
                .w_full()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(pal.foreground)
                .child(Icon::new(IconName::Loader).xsmall().text_color(pal.success))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(run.tool.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(pal.muted)
                        .child(detail),
                );
            if run.args_redacted {
                row = row.child(activity_chip("redacted", pal.muted));
            }
            if let Some(p) = &run.progress {
                row = row.child(div().text_color(pal.muted).child(p.clone()));
            }
            return Some(row.into_any_element());
        }

        card.recent_tools
            .back()
            .map(|run| self.tool_result_row(run, pal))
    }

    fn tool_result_row(&self, run: &ToolRun, pal: &Palette) -> AnyElement {
        let color = if run.is_error {
            pal.danger
        } else {
            pal.success
        };
        let label = if run.is_error { "error" } else { "done" };
        let detail = run
            .diff_stat
            .clone()
            .or_else(|| run.target.clone())
            .or_else(|| run.args.clone())
            .unwrap_or_default();

        let mut row = h_flex()
            .w_full()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(pal.foreground)
            .child(div().size_2().rounded_full().bg(color))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(run.tool.clone()),
            )
            .child(activity_chip(label, color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(pal.muted)
                    .child(detail),
            );

        if run.args_redacted {
            row = row.child(activity_chip("redacted", pal.muted));
        }
        if let Some(ms) = run.duration_ms {
            row = row.child(div().text_color(pal.muted).child(format!("{}ms", ms)));
        }
        if let Some(code) = run.exit_code {
            row = row.child(div().text_color(pal.muted).child(format!("exit {code}")));
        }

        row.into_any_element()
    }

    fn footer_row(&self, card: &AgentCard, pal: &Palette) -> impl IntoElement {
        let mut row = h_flex()
            .w_full()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(pal.muted)
            .child(div().child(fmt_ago(card.age_secs())));

        if let Some(sid) = &card.session_id {
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_align(TextAlign::Right)
                    .text_color(pal.foreground)
                    .child(format!("sid {sid}")),
            );
        }

        row
    }
}

fn context_bar(card: &AgentCard, model: &ModelInfo, pal: &Palette) -> Option<AnyElement> {
    let used = card.context_used?;
    match model.context_window {
        Some(window) if window > 0 => {
            let frac = (used as f64 / window as f64).clamp(0.0, 1.0) as f32;
            let color = usage_color(frac);
            let pct = (frac * 100.0).round() as u32;
            let label = format!("{} / {}  {}%", fmt_tokens(used), fmt_tokens(window), pct);
            Some(
                v_flex()
                    .w_full()
                    .gap_0p5()
                    .child(
                        div()
                            .w_full()
                            .h(px(4.))
                            .rounded_sm()
                            .bg(pal.muted.opacity(0.25))
                            .child(div().h_full().w(relative(frac)).rounded_sm().bg(color)),
                    )
                    .child(div().text_xs().text_color(pal.muted).child(label))
                    .into_any_element(),
            )
        }
        _ => Some(
            div()
                .text_xs()
                .text_color(pal.muted)
                .child(format!("{} tokens", fmt_tokens(used)))
                .into_any_element(),
        ),
    }
}

fn usage_color(frac: f32) -> Hsla {
    let t = frac.clamp(0.0, 1.0);
    let hue = (1.0 - t) * (120.0 / 360.0);
    gpui::hsla(hue, 0.9, 0.48, 1.0)
}

fn file_row(file: &FileEntry, pal: &Palette) -> AnyElement {
    let path_text = match &file.dest {
        Some(dest) => format!(
            "{} → {}",
            shorten_path(&file.path, 42),
            shorten_path(dest, 42)
        ),
        None => shorten_path(&file.path, 42),
    };

    h_flex()
        .w_full()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(pal.muted)
        .child(
            Icon::new(action_icon(file.action))
                .xsmall()
                .text_color(pal.muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(pal.foreground)
                .child(path_text),
        )
        .child(div().child(action_word(file.action)))
        .into_any_element()
}
