//! Agent card rendering (`docs/agent-panel-display.md` §5–§7).
//!
//! Card rendering lives here as `impl AgentListView` methods so it can attach
//! interaction listeners (click-to-focus, expand toggle) scoped to the panel
//! view. All colors come from the captured [`Palette`] (theme tokens only — §6,
//! §11); every free-text field was already sanitized on ingest (§10).

use gpui::{
    AnyElement, App, FontWeight, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _,
    px, relative,
};
use gpui_component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_state::{AgentCard, ApprovalInfo, FileEntry, Lifecycle, ToolRun};
use oneterm_terminal::{AgentState, ApprovalRisk, FileAction};

use crate::AgentListView;

/// Theme tokens captured once per render (all `Hsla` are `Copy`), so card
/// sub-renderers can be built while `cx` stays free for click listeners.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
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

/// The accent token for a card's current state/lifecycle.
pub(crate) fn state_accent(card: &AgentCard, pal: &Palette) -> Hsla {
    match card.lifecycle {
        Lifecycle::Ended { .. } => pal.muted,
        Lifecycle::Stale => pal.warning,
        Lifecycle::Live => match card.state {
            AgentState::Working => pal.success,
            AgentState::Blocked => pal.warning,
            AgentState::Error => pal.danger,
            AgentState::Idle | AgentState::Done => pal.muted,
        },
    }
}

/// Short state label for the card header.
fn state_label(card: &AgentCard) -> &'static str {
    match card.lifecycle {
        Lifecycle::Ended { .. } => "ended",
        Lifecycle::Stale => "stale",
        Lifecycle::Live => match card.state {
            AgentState::Working => "working",
            AgentState::Blocked => "blocked",
            AgentState::Idle => "idle",
            AgentState::Done => "done",
            AgentState::Error => "error",
        },
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

fn risk_color(risk: ApprovalRisk, pal: &Palette) -> Hsla {
    match risk {
        ApprovalRisk::Low => pal.info,
        ApprovalRisk::Medium => pal.warning,
        ApprovalRisk::High => pal.danger,
    }
}

fn risk_label(risk: ApprovalRisk) -> &'static str {
    match risk {
        ApprovalRisk::Low => "low",
        ApprovalRisk::Medium => "medium",
        ApprovalRisk::High => "high",
    }
}

// ── Formatting helpers ──────────────────────────────────────────────────

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

/// Truncate a session id for the footer chip.
fn sid_short(id: &str) -> String {
    let n = id.chars().count();
    if n <= 8 {
        id.to_string()
    } else {
        let head: String = id.chars().take(8).collect();
        format!("{head}…")
    }
}

fn chip(text: impl Into<SharedString>, fg: Hsla, bg: Hsla) -> impl IntoElement {
    div()
        .px_1p5()
        .rounded_sm()
        .bg(bg.opacity(0.18))
        .text_color(fg)
        .text_xs()
        .child(text.into())
}

impl AgentListView {
    /// Render one agent card (`docs/agent-panel-display.md` §5).
    pub(crate) fn render_card(
        &self,
        card: &AgentCard,
        pal: &Palette,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let accent = state_accent(card, pal);
        let key = (card.terminal_key, card.agent_id.clone());
        let expanded = self.is_card_expanded(&key);
        let card_id = SharedString::from(format!(
            "agent-card-{:?}-{}",
            card.terminal_key, card.agent_id
        ));
        let terminal_key = card.terminal_key;
        let dim =
            matches!(card.lifecycle, Lifecycle::Ended { .. }) || card.lifecycle == Lifecycle::Stale;

        let mut body = v_flex()
            .w_full()
            .gap_1()
            .child(self.card_header(card, pal, &key, expanded, cx));

        // Model chip + context-usage bar (§5.2).
        if let Some(model) = &card.model {
            body = body.child(self.model_row(card, model, pal));
        }
        // Approval banner when blocked (§5.5) — expanded automatically.
        if card.state == AgentState::Blocked {
            if let Some(approval) = &card.pending_approval {
                body = body.child(approval_banner(approval, pal));
            }
        }
        // Error message (§6: error → show message).
        if card.state == AgentState::Error {
            if let Some(msg) = &card.message {
                body = body.child(div().text_xs().text_color(pal.danger).child(msg.clone()));
            }
        }
        // Current / last tool row (§5.3).
        if let Some(row) = self.activity_row(card, pal) {
            body = body.child(row);
        }
        // Recent file feed (§5.4) — most recent last, show newest first.
        for file in card.recent_files.iter().rev() {
            body = body.child(file_row(file, pal));
        }
        // Footer (§5.6).
        body = body.child(self.footer_row(card, pal));

        // Expanded detail (§4 rule 3).
        if expanded {
            body = body.child(self.expanded_detail(card, pal));
        }

        v_flex()
            .id(card_id)
            .w_full()
            .p_2()
            .gap_1()
            .border_l_2()
            .border_color(accent)
            .bg(pal.tab_bar.opacity(0.35))
            .when(dim, |this| this.opacity(0.75))
            .cursor_pointer()
            .hover(|this| this.bg(pal.accent.opacity(0.12)))
            .on_click(cx.listener(move |_this, _ev, window, cx| {
                oneterm_state::agent_focus::focus_terminal(terminal_key, window, cx);
            }))
            .child(body)
            .into_any_element()
    }

    fn card_header(
        &self,
        card: &AgentCard,
        pal: &Palette,
        key: &(gpui::EntityId, String),
        expanded: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let accent = state_accent(card, pal);
        let (dot, dot_color) = liveness_dot(card, pal);
        let expander_id =
            SharedString::from(format!("expand-{:?}-{}", card.terminal_key, card.agent_id));
        let key = key.clone();

        h_flex()
            .w_full()
            .items_center()
            .gap_1p5()
            // State dot.
            .child(div().size_2().rounded_full().bg(accent))
            // Agent id (bold).
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(pal.foreground)
                    .child(card.agent_id.clone()),
            )
            // State label.
            .child(div().text_xs().text_color(accent).child(state_label(card)))
            // Space label (muted).
            .child(
                div()
                    .text_xs()
                    .text_color(pal.muted)
                    .child(card.space_label.clone()),
            )
            .child(div().flex_1())
            // Ended exit-code chip (§6).
            .when_some(ended_chip(card, pal), |this, el| this.child(el))
            // Liveness dot + word.
            .child(
                h_flex()
                    .items_center()
                    .gap_0p5()
                    .text_xs()
                    .text_color(dot_color)
                    .child(dot)
                    .child(div().text_color(pal.muted).child(liveness_word(card))),
            )
            // Expand/collapse chevron.
            .child(
                div()
                    .id(expander_id)
                    .size_4()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .rounded_sm()
                    .hover(|this| this.bg(pal.accent.opacity(0.2)))
                    .on_click(cx.listener(move |this, _ev, _window, cx| {
                        cx.stop_propagation();
                        this.toggle_card(key.clone());
                        cx.notify();
                    }))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .xsmall()
                        .text_color(pal.muted),
                    ),
            )
    }

    fn model_row(
        &self,
        card: &AgentCard,
        model: &oneterm_state::ModelInfo,
        pal: &Palette,
    ) -> impl IntoElement {
        let mut line = h_flex()
            .items_center()
            .gap_1()
            .text_xs()
            .text_color(pal.muted)
            .child(
                div()
                    .text_color(pal.foreground)
                    .child(model.display_name().to_string()),
            )
            .child(div().child("·"))
            .child(div().child(model.provider.clone()));
        if model.reasoning {
            line = line.child(chip("reasoning", pal.info, pal.info));
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
            // Running tool: spinner + tool + target/args (+ live progress).
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
                    Icon::new(IconName::LoaderCircle)
                        .xsmall()
                        .text_color(pal.success),
                )
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(run.tool.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(pal.muted)
                        .child(detail),
                );
            if run.args_redacted {
                row = row.child(chip("redacted", pal.muted, pal.muted));
            }
            if let Some(p) = &run.progress {
                row = row.child(div().text_color(pal.muted).child(p.clone()));
            }
            return Some(row.into_any_element());
        }
        // Last finished tool.
        card.recent_tools
            .back()
            .map(|run| tool_result_row(run, pal))
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
            row = row
                .child(div().child("·"))
                .child(div().child(format!("sid {}", sid_short(sid))));
        }
        if let Some(note) = &card.resolved_note {
            row = row
                .child(div().child("·"))
                .child(div().child(format!("resolved: {note}")));
        }
        row
    }

    fn expanded_detail(&self, card: &AgentCard, pal: &Palette) -> impl IntoElement {
        let mut col = v_flex()
            .w_full()
            .gap_1()
            .mt_1()
            .pt_1()
            .border_t_1()
            .border_color(pal.border);

        // Session lineage.
        if let Some(parent) = &card.parent_id {
            col = col.child(
                div()
                    .text_xs()
                    .text_color(pal.muted)
                    .child(format!("forked from {}", sid_short(parent))),
            );
        }
        if let Some(reason) = &card.session_reason {
            col = col.child(
                div()
                    .text_xs()
                    .text_color(pal.muted)
                    .child(format!("reason: {reason}")),
            );
        }

        // Full recent tool history (newest first).
        if !card.recent_tools.is_empty() {
            col = col.child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(pal.muted)
                    .child("tool history"),
            );
            for run in card.recent_tools.iter().rev() {
                col = col.child(tool_result_row(run, pal));
            }
        }

        col
    }
}

fn liveness_dot(card: &AgentCard, pal: &Palette) -> (SharedString, Hsla) {
    match card.lifecycle {
        Lifecycle::Live => (SharedString::from("●"), pal.success),
        Lifecycle::Stale => (SharedString::from("◐"), pal.warning),
        Lifecycle::Ended { .. } => (SharedString::from("○"), pal.muted),
    }
}

fn liveness_word(card: &AgentCard) -> &'static str {
    match card.lifecycle {
        Lifecycle::Live => "live",
        Lifecycle::Stale => "stale",
        Lifecycle::Ended { .. } => "ended",
    }
}

fn ended_chip(card: &AgentCard, pal: &Palette) -> Option<AnyElement> {
    match card.lifecycle {
        Lifecycle::Ended { exit_code } => {
            let (label, color) = match exit_code {
                Some(0) | None => (
                    exit_code
                        .map(|c| format!("exit {c}"))
                        .unwrap_or_else(|| "exit 0".to_string()),
                    pal.success,
                ),
                Some(c) => (format!("exit {c}"), pal.danger),
            };
            Some(chip(label, color, color).into_any_element())
        }
        _ => None,
    }
}

fn context_bar(
    card: &AgentCard,
    model: &oneterm_state::ModelInfo,
    pal: &Palette,
) -> Option<AnyElement> {
    let used = card.context_used?;
    match model.context_window {
        Some(window) if window > 0 => {
            let frac = (used as f64 / window as f64).clamp(0.0, 1.0) as f32;
            let color = if frac >= 0.9 {
                pal.danger
            } else if frac >= 0.7 {
                pal.warning
            } else {
                pal.success
            };
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
        // Only the used-tokens count is known.
        _ => Some(
            div()
                .text_xs()
                .text_color(pal.muted)
                .child(format!("{} tokens", fmt_tokens(used)))
                .into_any_element(),
        ),
    }
}

fn tool_result_row(run: &ToolRun, pal: &Palette) -> AnyElement {
    let (mark, mark_color) = if run.is_error {
        ("✗", pal.danger)
    } else {
        ("✓", pal.success)
    };
    let mut inner = h_flex()
        .w_full()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(pal.muted)
        .child(div().text_color(mark_color).child(mark))
        .child(div().text_color(pal.foreground).child(run.tool.clone()));
    if let Some(target) = &run.target {
        inner = inner.child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_ellipsis()
                .child(target.clone()),
        );
    } else {
        inner = inner.child(div().flex_1());
    }
    if let Some(diff) = &run.diff_stat {
        inner = inner.child(div().text_color(pal.foreground).child(diff.clone()));
    }
    if let Some(ms) = run.duration_ms {
        inner = inner.child(div().child(fmt_duration(ms)));
    }

    let mut col = v_flex().w_full().gap_0p5().child(inner);
    if run.is_error {
        if let Some(err) = &run.error_message {
            col = col.child(div().text_xs().text_color(pal.danger).child(err.clone()));
        }
    }
    col.into_any_element()
}

fn fmt_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

fn file_row(file: &FileEntry, pal: &Palette) -> AnyElement {
    let path_text = match &file.dest {
        Some(dest) => format!("{} → {}", file.path, dest),
        None => file.path.clone(),
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
                .overflow_hidden()
                .text_ellipsis()
                .text_color(pal.foreground)
                .child(path_text),
        )
        .child(div().child(action_word(file.action)))
        .into_any_element()
}

fn approval_banner(approval: &ApprovalInfo, pal: &Palette) -> AnyElement {
    let accent = approval
        .risk
        .map(|r| risk_color(r, pal))
        .unwrap_or(pal.warning);

    let mut header = h_flex().items_center().gap_1().child(
        Icon::new(IconName::TriangleAlert)
            .xsmall()
            .text_color(accent),
    );
    if let Some(risk) = approval.risk {
        header = header.child(chip(risk_label(risk), accent, accent));
    }
    header = header.child(
        div()
            .flex_1()
            .text_xs()
            .text_color(pal.foreground)
            .child(approval.prompt.clone()),
    );

    // Option buttons (display-only in v1 — §7 Tier 2 is opt-in / out of scope).
    let options: Vec<String> = if let Some(choices) = &approval.choices {
        choices.iter().map(|c| c.label.clone()).collect()
    } else if let Some(opts) = &approval.options {
        opts.clone()
    } else {
        vec!["yes".to_string(), "no".to_string()]
    };
    let default = approval.default.clone();

    let mut buttons = h_flex().flex_wrap().gap_1();
    for opt in options {
        let is_default = default.as_deref() == Some(opt.as_str());
        let label = if is_default {
            format!("{opt}*")
        } else {
            opt.clone()
        };
        buttons = buttons.child(
            div()
                .px_1p5()
                .py_0p5()
                .rounded_sm()
                .border_1()
                .border_color(pal.border)
                .text_xs()
                .text_color(pal.foreground)
                .when(is_default, |this| this.border_color(accent))
                .child(label),
        );
    }

    let mut col = v_flex()
        .w_full()
        .gap_1()
        .p_1p5()
        .rounded_sm()
        .bg(accent.opacity(0.12))
        .border_1()
        .border_color(accent.opacity(0.5))
        .child(header)
        .child(buttons);

    // Countdown hint (§5.5).
    if let (Some(timeout), Some(def)) = (approval.timeout_ms, &default) {
        if timeout > 0 {
            col = col.child(div().text_xs().text_color(pal.muted).child(format!(
                "auto {} in {}s",
                def,
                timeout / 1000
            )));
        }
    }
    // v1: buttons are display-only — the user answers in the terminal.
    col = col.child(
        div()
            .text_xs()
            .text_color(pal.muted)
            .child("waiting for your input in the terminal"),
    );
    col.into_any_element()
}
