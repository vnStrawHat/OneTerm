//! Agent card rendering (`docs/agent-panel-display.md` §5–§7).
//!
//! This module keeps each card compact: a single header line, one model line,
//! and a footer that keeps the full session id visible until layout constraints
//! force truncation.

use gpui::{
    AnyElement, App, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TextAlign, div,
    prelude::FluentBuilder as _, px, relative,
};
use gpui_component::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};

use oneterm_state::{AgentCard, Lifecycle, ModelInfo, ToolRun};
use oneterm_terminal::AgentState;

use crate::AgentListView;

/// Theme tokens captured once per render (all `Hsla` are `Copy`), so card
/// sub-renderers can be built while `cx` stays free for click listeners.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) magenta: Hsla,
    pub(crate) success: Hsla,
    pub(crate) warning: Hsla,
    pub(crate) danger: Hsla,
    pub(crate) info: Hsla,
    pub(crate) accent: Hsla,
    pub(crate) muted: Hsla,
    pub(crate) foreground: Hsla,
    pub(crate) background: Hsla,
    pub(crate) border: Hsla,
    pub(crate) tab_bar: Hsla,
}

impl Palette {
    pub(crate) fn capture(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            magenta: theme.magenta,
            success: theme.success,
            warning: theme.warning,
            danger: theme.danger,
            info: theme.info,
            accent: theme.accent,
            muted: theme.muted_foreground,
            foreground: theme.foreground,
            background: theme.background,
            border: theme.border,
            tab_bar: *theme.tokens.tab_bar,
        }
    }
}

/// The accent token for a card's current agent state.
fn state_accent(card: &AgentCard, pal: &Palette) -> Hsla {
    state_color(card.state, pal)
}

fn state_color(state: AgentState, pal: &Palette) -> Hsla {
    match state {
        AgentState::Working => pal.success,
        AgentState::Blocked => pal.warning,
        AgentState::Idle => pal.muted,
        AgentState::Done => pal.info,
        AgentState::Error => pal.danger,
    }
}

fn lifecycle_color(card: &AgentCard, pal: &Palette) -> Hsla {
    match card.lifecycle {
        oneterm_state::Lifecycle::Live => pal.success,
        oneterm_state::Lifecycle::Stale => pal.warning,
        oneterm_state::Lifecycle::Ended { .. } => pal.muted,
    }
}

/// Short state label for the card header.
fn state_label(state: AgentState) -> &'static str {
    match state {
        AgentState::Working => " working",
        AgentState::Blocked => " blocked",
        AgentState::Idle => " idle",
        AgentState::Done => " done",
        AgentState::Error => " error",
    }
}

fn liveness_word(card: &AgentCard) -> &'static str {
    match card.lifecycle {
        oneterm_state::Lifecycle::Live => "live",
        oneterm_state::Lifecycle::Stale => "stale",
        oneterm_state::Lifecycle::Ended { .. } => "ended",
    }
}

fn working_spinner_frame(card: &AgentCard) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let ix = (card.last_recv.elapsed().as_millis() / 120) as usize % FRAMES.len();
    FRAMES[ix]
}

fn state_indicator(card: &AgentCard, pal: &Palette) -> AnyElement {
    let color = state_color(card.state, pal);
    let marker = if card.state == AgentState::Working {
        div()
            .w_2()
            .text_sm()
            .font_weight(FontWeight::BOLD)
            .text_color(color)
            .child(working_spinner_frame(card))
            .into_any_element()
    } else {
        div().size_2().rounded_full().bg(color).into_any_element()
    };

    h_flex()
        .items_center()
        .gap_1()
        .text_sm()
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(marker)
        .child(state_label(card.state))
        .into_any_element()
}

fn lifecycle_summary(card: &AgentCard, pal: &Palette) -> AnyElement {
    let color = lifecycle_color(card, pal);
    let text = format!(
        "{} #{}: {}",
        card.agent_id,
        card.space_number,
        liveness_word(card)
    );

    h_flex()
        .flex_1()
        .min_w_0()
        .justify_end()
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(text),
        )
        .into_any_element()
}

/// Format a token count compactly: `84500 → 84.5k`, `200000 → 200k`.
fn fmt_tokens(n: u64) -> String {
    let (v, suffix) = if n >= 1_000_000 {
        (n as f64 / 1e6, "M")
    } else if n >= 1_000 {
        (n as f64 / 1e3, "k")
    } else {
        return n.to_string();
    };
    if v.fract().abs() < 0.05 {
        format!("{v:.0}{suffix}")
    } else {
        format!("{v:.1}{suffix}")
    }
}

/// Relative-time label from an age in seconds.
fn fmt_ago(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

fn activity_chip(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .px_1p5()
        .rounded_sm()
        .bg(color.opacity(0.18))
        .text_color(color)
        .text_xs()
        .child(text.into())
}

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
            let use_start_ellipsis = run.target.is_some();
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
                        .when(use_start_ellipsis, |this| this.text_ellipsis_start())
                        .when(!use_start_ellipsis, |this| this.text_ellipsis())
                        .text_color(pal.muted)
                        .child(detail),
                );
            if run.args_redacted {
                row = row.child(activity_chip("redacted", pal.muted));
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

        let use_start_ellipsis = run.target.is_some();
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
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(color)
                    .child(if run.is_error { "✕" } else { "✓" }),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(color)
                    .child(run.tool.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .when(use_start_ellipsis, |this| this.text_ellipsis_start())
                    .when(!use_start_ellipsis, |this| this.text_ellipsis())
                    .text_color(pal.muted)
                    .child(detail),
            );

        if let Some(ms) = run.duration_ms {
            row = row.child(div().text_color(pal.muted).child(fmt_duration(ms)));
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

fn fmt_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else {
        let secs = ms as f64 / 1_000.0;
        if secs.fract().abs() < 0.05 {
            format!("{secs:.0}s")
        } else {
            format!("{secs:.1}s")
        }
    }
}

fn context_bar(card: &AgentCard, model: &ModelInfo, pal: &Palette) -> Option<AnyElement> {
    let used = card.context_used?;
    let project_dir = card.project_dir.as_deref().map(fmt_project_dir);

    match model.context_window {
        Some(window) if window > 0 => {
            let frac = (used as f64 / window as f64).clamp(0.0, 1.0) as f32;
            let color = usage_color(frac, pal);
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
                    .child(context_info_row(label, project_dir.as_deref(), pal))
                    .into_any_element(),
            )
        }
        _ => Some(
            context_info_row(
                format!("{} tokens", fmt_tokens(used)),
                project_dir.as_deref(),
                pal,
            )
            .into_any_element(),
        ),
    }
}

fn context_info_row(label: String, project_dir: Option<&str>, pal: &Palette) -> impl IntoElement {
    let mut row = h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .justify_between()
        .text_xs()
        .text_color(pal.muted)
        .child(div().flex_shrink_0().min_w_0().child(label));

    if let Some(project_dir) = project_dir {
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_align(TextAlign::Right)
                .child(project_dir.to_string()),
        );
    }

    row
}

fn fmt_project_dir(path: &str) -> String {
    const MAX_LEN: usize = 45;
    if path.chars().count() <= MAX_LEN {
        return path.to_string();
    }

    let uses_windows_backslash = matches!(
        path.as_bytes(),
        [drive, b':', b'\\', ..] if drive.is_ascii_alphabetic()
    );
    let normalized = path.replace('\\', "/");
    let (mut prefix, rest) = split_project_dir_prefix(&normalized);
    if uses_windows_backslash && prefix.ends_with(":/") {
        prefix = format!("{}:\\", &prefix[..1]);
    }

    let segments: Vec<&str> = rest
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() < 3 {
        return ellipsize_path_middle(path, MAX_LEN);
    }

    let last_two = format!(
        "{}/{}",
        segments[segments.len() - 2],
        segments[segments.len() - 1]
    );
    let candidate = if segments.len() > 3 {
        format!("{}{}/.../{}", prefix, segments[0], last_two)
    } else {
        format!("{}.../{}", prefix, last_two)
    };
    if candidate.chars().count() <= MAX_LEN {
        return candidate;
    }

    let candidate = format!("{}.../{}", prefix, last_two);
    if candidate.chars().count() <= MAX_LEN {
        return candidate;
    }

    let budget = MAX_LEN.saturating_sub(prefix.chars().count() + 4);
    format!("{}.../{}", prefix, truncate_tail_chars(&last_two, budget))
}

fn split_project_dir_prefix(path: &str) -> (String, &str) {
    if let Some(rest) = path.strip_prefix('/') {
        return ("/".to_string(), rest);
    }

    if path.len() >= 3 {
        let bytes = path.as_bytes();
        if bytes[1] == b':' && matches!(bytes[0], b'A'..=b'Z' | b'a'..=b'z') {
            if let Some(rest) = path[2..].strip_prefix('/') {
                let prefix = format!("{}:/", &path[..1]);
                return (prefix, rest);
            }
        }
    }

    (String::new(), path)
}

fn ellipsize_path_middle(path: &str, max_len: usize) -> String {
    let count = path.chars().count();
    if count <= max_len {
        return path.to_string();
    }
    if max_len <= 3 {
        return "...".chars().take(max_len).collect();
    }

    let keep_tail = max_len - 3;
    let tail: String = path
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}

fn truncate_tail_chars(text: &str, max_len: usize) -> String {
    let count = text.chars().count();
    if count <= max_len {
        return text.to_string();
    }
    if max_len <= 3 {
        return "...".chars().take(max_len).collect();
    }

    let keep_tail = max_len - 3;
    let tail: String = text
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}

/// Context-usage tint: success at 0 %, warning at 50 %, danger at 100 % —
/// derived from the theme rather than a fixed hue ramp (HYG-13).
fn usage_color(frac: f32, pal: &Palette) -> Hsla {
    let t = frac.clamp(0.0, 1.0);
    if t <= 0.5 {
        pal.success.mix_oklab(pal.warning, t * 2.0)
    } else {
        pal.warning.mix_oklab(pal.danger, (t - 0.5) * 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_project_dir;

    #[test]
    fn keeps_short_project_dirs_unchanged() {
        assert_eq!(
            fmt_project_dir(r"D:\TrungKFC-Research\Rust\myTerm2"),
            r"D:\TrungKFC-Research\Rust\myTerm2"
        );
        assert_eq!(
            fmt_project_dir("/opt/app/dev/myProject"),
            "/opt/app/dev/myProject"
        );
    }

    #[test]
    fn formats_long_project_dirs_with_middle_ellipsis() {
        assert_eq!(
            fmt_project_dir(r"D:\TrungKFC-Research\some-very-long-folder\Rust\myTerm2"),
            r"D:\TrungKFC-Research/.../Rust/myTerm2"
        );
        assert_eq!(
            fmt_project_dir("/opt/application-source-tree/very-long-middle/dev/myProject"),
            "/opt/.../dev/myProject"
        );
    }

    #[test]
    fn caps_very_long_project_dir() {
        assert!(
            fmt_project_dir("/very-long-root/application/dev/some-extremely-long-project-name")
                .chars()
                .count()
                <= 45
        );
    }
}
