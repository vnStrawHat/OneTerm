//! Agent card rendering (`docs/agent-panel-display.md` §5–§7).
//!
//! This module keeps each card compact: a single header line, one model line,
//! and a footer that keeps the full session id visible until layout constraints
//! force truncation.

mod render;

use gpui::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _,
    div,
};
use gpui_component::{ActiveTheme as _, h_flex};

use oneterm_state::AgentCard;
use oneterm_terminal::AgentState;

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

fn activity_chip(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .px_1p5()
        .rounded_sm()
        .bg(color.opacity(0.18))
        .text_color(color)
        .text_xs()
        .child(text.into())
}
