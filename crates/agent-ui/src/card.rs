//! Agent card rendering (`docs/agent-panel-display.md` §5–§7).
//!
//! This module keeps each card compact: a single header line, one model line,
//! the latest file row, and a footer that keeps the full session id visible
//! until layout constraints force truncation.

use gpui::{
    AnyElement, App, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TextAlign, div,
    prelude::FluentBuilder as _, px, relative,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_state::{AgentCard, FileEntry, Lifecycle, ModelInfo};
use oneterm_terminal::{AgentState, FileAction};

use crate::AgentListView;

/// Theme tokens captured once per render (all `Hsla` are `Copy`), so card
/// sub-renderers can be built while `cx` stays free for click listeners.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub magenta: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub info: Hsla,
    pub accent: Hsla,
    pub muted: Hsla,
    pub foreground: Hsla,
    pub background: Hsla,
    pub border: Hsla,
    pub tab_bar: Hsla,
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

// ── Visual mapping (§6) ─────────────────────────────────────────────────

/// The accent token for a card's current agent state.
pub(crate) fn state_accent(card: &AgentCard, pal: &Palette) -> Hsla {
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
        Lifecycle::Live => pal.success,
        Lifecycle::Stale => pal.warning,
        Lifecycle::Ended { .. } => pal.muted,
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
        Lifecycle::Live => "live",
        Lifecycle::Stale => "stale",
        Lifecycle::Ended { .. } => "ended",
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
        "{} {}: {}",
        card.agent_id,
        space_label_text(&card.space_label),
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

fn space_label_text(label: &str) -> String {
    if label == "single" {
        "#0".to_string()
    } else {
        label.to_string()
    }
}

fn action_icon(action: FileAction) -> IconName {
    match action {
        FileAction::Read => IconName::Eye,
        FileAction::Edit => IconName::Replace,
        FileAction::Write | FileAction::Create => IconName::File,
        FileAction::Delete => IconName::Delete,
        FileAction::Move => IconName::ArrowRight,
    }
}

fn action_word(action: FileAction) -> &'static str {
    match action {
        FileAction::Read => "read",
        FileAction::Edit => "edit",
        FileAction::Write => "write",
        FileAction::Create => "create",
        FileAction::Delete => "delete",
        FileAction::Move => "move",
    }
}

/// Format a token count compactly: `84500 → 84.5k`, `200000 → 200k`.
pub(crate) fn fmt_tokens(n: u64) -> String {
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
pub(crate) fn fmt_ago(secs: u64) -> String {
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

fn ellipsize_left(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return "...".chars().take(max_chars).collect();
    }

    let keep = max_chars - 3;
    let tail: String = text
        .chars()
        .rev()
        .take(keep)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}

fn shorten_path(path: &str, max_chars: usize) -> String {
    let count = path.chars().count();
    if count <= max_chars {
        return path.to_string();
    }

    let sep = if path.contains('\\') { '\\' } else { '/' };
    let components: Vec<String> = path
        .split(|ch| ch == '/' || ch == '\\')
        .filter(|part| !(part.is_empty() || (part.len() == 2 && part.ends_with(':'))))
        .map(ToOwned::to_owned)
        .collect();

    if components.is_empty() {
        return ellipsize_left(path, max_chars);
    }

    let mut suffix: Vec<String> = vec![components.last().cloned().unwrap_or_default()];
    let mut suffix_len = suffix[0].chars().count();
    let prefix_len = 4; // ".../"

    for part in components[..components.len() - 1].iter().rev() {
        let part_len = part.chars().count();
        let next_len = suffix_len + 1 + part_len;
        if prefix_len + next_len > max_chars {
            break;
        }
        suffix.push(part.clone());
        suffix_len = next_len;
    }
    suffix.reverse();

    let joined = suffix.join(&sep.to_string());
    if prefix_len + joined.chars().count() <= max_chars {
        format!("...{sep}{joined}")
    } else {
        ellipsize_left(&joined, max_chars)
    }
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
