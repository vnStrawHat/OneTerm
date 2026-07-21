//! OneTerm Agent feature — the **Agent Panel** content.
//!
//! A right-dock "fleet view" of coding agents running inside terminals. Agents
//! report status over OSC 9;7 (`docs/osc-agent-status.md`); `terminal-view`
//! folds those events into a global [`oneterm_state::AgentRegistry`], and this
//! crate renders it: a scrolling column of tab groups, each holding one card per
//! `(terminal, agent)`. See `docs/agent-panel-display.md`.
//!
//! Layering: this is a **feature crate** (crate rule R5) — it depends only on
//! shared layers (`state`, `terminal`, `settings`, `theme`, `ui`) and never on
//! another feature. The `app` crate composes [`AgentListView`] into the dock
//! panel (R9), mirroring how `SshClientPanel` composes `SessionPanel`.

mod card;

use std::time::Duration;

use gpui::{
    App, AppContext as _, Context, Entity, EntityId, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, div, prelude::FluentBuilder as _,
};
use gpui_component::{Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_settings::UiConfig;
use oneterm_state::{AgentCard, AgentRegistry, AgentStateCounts, Lifecycle};
use oneterm_terminal::AgentState;

use card::Palette;

/// How often the view re-renders relative-time labels and card spinners.
const RELATIVE_TIME_TICK: Duration = Duration::from_millis(120);

/// How often the registry is polled to mark idle cards stale (§9).
const STALE_TICK: Duration = Duration::from_secs(15);

/// Card-state filter for the header chips (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Working,
    Blocked,
    Errors,
    Idle,
    Done,
}

#[derive(Clone, Copy)]
struct StatusChipSpec {
    filter: Filter,
    marker: &'static str,
    label: &'static str,
    count: usize,
    color: gpui::Hsla,
}

/// The Agent Panel content view. Observes the global [`AgentRegistry`] and
/// re-renders on registry changes and on a periodic tick for relative-time
/// labels; owns the view-local filter state.
pub struct AgentListView {
    focus_handle: FocusHandle,
    filter: Filter,
    _subs: Vec<gpui::Subscription>,
    _refresh_task: Task<()>,
}

impl AgentListView {
    /// Create the view: ensure the registry exists, wire the config-driven stale
    /// threshold, and start the periodic relative-time / stale refresh tick.
    pub fn new(_window: &mut gpui::Window, cx: &mut Context<Self>) -> Self {
        AgentRegistry::init(cx);
        let registry = AgentRegistry::global(cx);

        // Push the initial stale threshold from UiConfig and keep it in sync.
        let ui = UiConfig::global(cx);
        let threshold = ui.read(cx).agent_stale_threshold_ms();
        registry.update(cx, |reg, cx| reg.set_stale_threshold_ms(threshold, cx));

        let mut subs = Vec::new();
        subs.push(cx.observe(&registry, |_, _, cx| cx.notify()));
        subs.push(cx.observe(&ui, |_, ui, cx| {
            let ms = ui.read(cx).agent_stale_threshold_ms();
            AgentRegistry::global(cx).update(cx, |reg, cx| reg.set_stale_threshold_ms(ms, cx));
        }));

        let refresh_task = cx.spawn(async move |this, cx| {
            let mut stale_elapsed = Duration::ZERO;
            loop {
                cx.background_executor().timer(RELATIVE_TIME_TICK).await;
                stale_elapsed += RELATIVE_TIME_TICK;

                let alive = this
                    .update(cx, |_, cx| {
                        if stale_elapsed >= STALE_TICK {
                            stale_elapsed = Duration::ZERO;
                            if let Some(reg) = AgentRegistry::try_global(cx) {
                                reg.update(cx, |reg, cx| reg.refresh_stale(cx));
                            }
                        }

                        // Relative-time labels are derived from `Instant::elapsed()`
                        // during render, so they need a view refresh even when no
                        // registry state changed.
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            filter: Filter::All,
            _subs: subs,
            _refresh_task: refresh_task,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut gpui::Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    // ── view-local state ────────────────────────────────────────────────

    fn passes_filter(&self, card: &AgentCard) -> bool {
        if matches!(card.lifecycle, Lifecycle::Ended { .. }) {
            return false;
        }
        match self.filter {
            Filter::All => true,
            Filter::Working => card.state == AgentState::Working,
            Filter::Blocked => card.state == AgentState::Blocked,
            Filter::Errors => card.state == AgentState::Error,
            Filter::Idle => card.state == AgentState::Idle,
            Filter::Done => card.state == AgentState::Done,
        }
    }

    // ── rendering ───────────────────────────────────────────────────────

    fn render_header(
        &self,
        counts: &AgentStateCounts,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = h_flex()
            .w_full()
            .items_center()
            .gap_1()
            .child(Icon::new(IconName::Bot).small().text_color(pal.foreground))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_sm()
                    .text_color(pal.foreground)
                    .child("Agents"),
            )
            .child(div().flex_1());

        v_flex()
            .w_full()
            .flex_shrink_0()
            .px_2()
            .py_1()
            .gap_1()
            .bg(pal.tab_bar)
            .border_b_1()
            .border_color(pal.border)
            .child(title)
            .child(self.filter_chips(counts, pal, cx))
    }

    fn filter_chips(
        &self,
        counts: &AgentStateCounts,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut row = h_flex().w_full().flex_wrap().gap_1();
        let chips = [
            StatusChipSpec {
                filter: Filter::All,
                marker: "#",
                label: "All",
                count: counts.total,
                color: pal.magenta,
            },
            StatusChipSpec {
                filter: Filter::Working,
                marker: "⠋",
                label: "Work",
                count: counts.working,
                color: pal.success,
            },
            StatusChipSpec {
                filter: Filter::Blocked,
                marker: "▲",
                label: "Block",
                count: counts.blocked,
                color: pal.warning,
            },
            StatusChipSpec {
                filter: Filter::Errors,
                marker: "✕",
                label: "Err",
                count: counts.error,
                color: pal.danger,
            },
            StatusChipSpec {
                filter: Filter::Idle,
                marker: "○",
                label: "Idle",
                count: counts.idle,
                color: pal.muted,
            },
            StatusChipSpec {
                filter: Filter::Done,
                marker: "✓",
                label: "Done",
                count: counts.done,
                color: pal.info,
            },
        ];

        for chip in chips {
            row = row.child(self.filter_chip(chip, pal, cx));
        }
        row
    }

    fn filter_chip(
        &self,
        chip: StatusChipSpec,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.filter == chip.filter;
        let id = SharedString::from(format!("agent-filter-{}", chip.label));
        h_flex()
            .id(id)
            .items_center()
            .gap_1()
            .px_1p5()
            .py_0p5()
            .rounded_sm()
            .cursor_pointer()
            .text_xs()
            .border_1()
            .border_color(chip.color)
            .text_color(pal.foreground)
            .when(active, |this| this.bg(chip.color.opacity(0.18)))
            .hover(|this| this.bg(chip.color.opacity(0.12)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter = chip.filter;
                cx.notify();
            }))
            .child(
                div()
                    .text_color(chip.color)
                    .font_weight(FontWeight::BOLD)
                    .child(chip.marker),
            )
            .child(div().child(chip.label))
            .child(
                div()
                    .px_1()
                    .rounded_sm()
                    .bg(chip.color.opacity(0.16))
                    .text_color(pal.foreground)
                    .font_weight(FontWeight::BOLD)
                    .child(chip.count.to_string()),
            )
    }

    fn render_group(
        &self,
        tab_key: EntityId,
        tab_title: &str,
        cards: &[AgentCard],
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let group_id = SharedString::from(format!("agent-group-{tab_key:?}"));

        let header = h_flex()
            .id(group_id)
            .w_full()
            .items_center()
            .gap_1()
            .py_0p5()
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_weight(FontWeight::MEDIUM)
                    .text_sm()
                    .text_color(pal.foreground)
                    .child(tab_title.to_string()),
            )
            .children(group_badges(cards, pal));

        let mut col = v_flex().w_full().gap_1().child(header);
        for card in cards {
            col = col.child(self.render_card(card, pal, cx));
        }
        col
    }
}

impl Focusable for AgentListView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentListView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::capture(cx);
        let registry = AgentRegistry::global(cx);
        let (cards, counts) = {
            let reg = registry.read(cx);
            (reg.cards().to_vec(), reg.summary())
        };

        // Empty state (§4 rule 4).
        if cards.is_empty() {
            return v_flex()
                .id("agent-list-empty")
                .size_full()
                .track_focus(&self.focus_handle)
                .bg(pal.background)
                .items_center()
                .justify_center()
                .gap_1()
                .child(Icon::new(IconName::Bot).large().text_color(pal.muted))
                .child(
                    div()
                        .text_sm()
                        .text_color(pal.foreground)
                        .child("No agents reporting"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(pal.muted)
                        .child("Agents that emit OSC 9;7 appear here."),
                )
                .into_any_element();
        }

        // Group by tab_key, preserving first-seen order; filter + sort within.
        let mut order: Vec<EntityId> = Vec::new();
        let mut titles: Vec<(EntityId, String)> = Vec::new();
        let mut grouped: Vec<(EntityId, Vec<AgentCard>)> = Vec::new();
        for card in cards.into_iter().filter(|c| self.passes_filter(c)) {
            let tab_key = card.tab_key;
            let pos = match order.iter().position(|k| *k == tab_key) {
                Some(i) => i,
                None => {
                    order.push(tab_key);
                    titles.push((tab_key, card.tab_title.clone()));
                    grouped.push((tab_key, Vec::new()));
                    grouped.len() - 1
                }
            };
            grouped[pos].1.push(card);
        }
        for (_, group) in grouped.iter_mut() {
            group.sort_by(|a, b| {
                a.sort_rank()
                    .cmp(&b.sort_rank())
                    .then(b.last_recv.cmp(&a.last_recv))
            });
        }

        let mut list = v_flex()
            .id("agent-scroll")
            .w_full()
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .gap_2();
        for (i, (tab_key, group)) in grouped.iter().enumerate() {
            let title = titles[i].1.clone();
            list = list.child(self.render_group(*tab_key, &title, group, &pal, cx));
        }

        v_flex()
            .id("agent-list")
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(pal.background)
            .child(self.render_header(&counts, &pal, cx))
            .child(list)
            .into_any_element()
    }
}

// ── free rendering helpers ──────────────────────────────────────────────

/// Aggregate per-state count badges for a tab-group header (e.g. `🟢2 🟠1`).
fn group_badges(cards: &[AgentCard], pal: &Palette) -> Vec<gpui::AnyElement> {
    let mut working = 0;
    let mut blocked = 0;
    let mut error = 0;
    let mut resting = 0;
    for c in cards {
        if matches!(c.lifecycle, Lifecycle::Ended { .. }) {
            resting += 1;
            continue;
        }
        match c.state {
            AgentState::Working => working += 1,
            AgentState::Blocked => blocked += 1,
            AgentState::Error => error += 1,
            AgentState::Idle | AgentState::Done => resting += 1,
        }
    }
    let mut out: Vec<gpui::AnyElement> = Vec::new();
    let mut push = |n: usize, color: gpui::Hsla| {
        if n > 0 {
            out.push(
                h_flex()
                    .items_center()
                    .gap_0p5()
                    .text_xs()
                    .text_color(color)
                    .child(div().size_2().rounded_full().bg(color))
                    .child(div().child(n.to_string()))
                    .into_any_element(),
            );
        }
    };
    push(working, pal.success);
    push(blocked, pal.warning);
    push(error, pal.danger);
    push(resting, pal.muted);
    out
}

/// Initialize the Agent feature. Ensures the `AgentRegistry` global exists so
/// terminals can fold into it even before the panel is first opened.
pub fn init(cx: &mut App) {
    AgentRegistry::init(cx);
}
