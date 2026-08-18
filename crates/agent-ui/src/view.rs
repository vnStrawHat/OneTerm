use std::{collections::HashMap, time::Duration};

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, div, prelude::FluentBuilder as _,
};
use gpui_component::dock::{Panel, PanelControl, PanelEvent};
use gpui_component::{Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_settings::UiConfig;
use oneterm_state::{AgentCard, AgentRegistry, AgentStateCounts, Lifecycle, panel_names};
use oneterm_terminal::AgentState;

use crate::card::Palette;

/// How often active cards advance their spinner animation.
const ACTIVE_CARD_TICK: Duration = Duration::from_millis(120);

/// How often inactive cards refresh their relative-time labels.
const RELATIVE_TIME_TICK: Duration = Duration::from_secs(1);

/// How often the registry is polled to mark idle cards stale (§9).
const STALE_TICK: Duration = Duration::from_secs(15);

/// Card-state filter for the header chips (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    All,
    Working,
    Blocked,
    Errors,
    Idle,
    Done,
}

/// Stable display order for one terminal group, rebuilt only on registry changes.
struct AgentDisplayGroup {
    tab_key: gpui::EntityId,
    tab_title: String,
    card_indices: Vec<usize>,
}

fn build_display_groups(cards: &[AgentCard]) -> Vec<AgentDisplayGroup> {
    let mut group_indices = HashMap::new();
    let mut groups: Vec<AgentDisplayGroup> = Vec::new();
    for (card_index, card) in cards.iter().enumerate() {
        let group_index = match group_indices.get(&card.tab_key).copied() {
            Some(index) => index,
            None => {
                let index = groups.len();
                group_indices.insert(card.tab_key, index);
                groups.push(AgentDisplayGroup {
                    tab_key: card.tab_key,
                    tab_title: card.tab_title.clone(),
                    card_indices: Vec::new(),
                });
                index
            }
        };
        groups[group_index].card_indices.push(card_index);
    }
    for group in &mut groups {
        // Fixed within-tab order keyed by the Space's depth-first index, so cards
        // no longer jump around as agent state or event-arrival changes. Ties
        // (several agents sharing one Space) keep first-seen registry order via
        // the ascending insertion index.
        group.card_indices.sort_by(|&a, &b| {
            cards[a]
                .space_order
                .cmp(&cards[b].space_order)
                .then(a.cmp(&b))
        });
    }
    groups
}

/// The Agent Panel content view. Observes the global [`AgentRegistry`] and
/// re-renders on registry changes and on a periodic tick for relative-time
/// labels; owns the view-local filter state.
pub struct AgentListView {
    focus_handle: FocusHandle,
    filter: Filter,
    /// Cached registry data. Animation ticks must not clone the registry.
    cards: Vec<AgentCard>,
    groups: Vec<AgentDisplayGroup>,
    counts: AgentStateCounts,
    has_working_cards: bool,
    _subs: Vec<gpui::Subscription>,
    _refresh_task: Task<()>,
}

impl AgentListView {
    /// Create the view: ensure the registry exists, wire the config-driven stale
    /// threshold, and start the periodic relative-time / stale refresh tick.
    pub fn new(_window: &mut gpui::Window, cx: &mut Context<Self>) -> Self {
        AgentRegistry::init(cx);
        let registry = AgentRegistry::global(cx);

        let ui = UiConfig::global(cx);
        let threshold = ui.read(cx).agent_stale_threshold_ms();
        registry.update(cx, |reg, cx| reg.set_stale_threshold_ms(threshold, cx));

        let (cards, counts) = {
            let reg = registry.read(cx);
            (reg.cards().to_vec(), reg.summary())
        };
        let has_working_cards = cards.iter().any(card_is_working);
        let groups = build_display_groups(&cards);

        let mut subs = Vec::new();
        subs.push(cx.observe(&registry, |this, registry, cx| {
            // Clone the model only when the registry changes. Animation and
            // relative-time ticks render the cached snapshot below.
            let (cards, counts) = {
                let reg = registry.read(cx);
                (reg.cards().to_vec(), reg.summary())
            };
            this.cards = cards;
            this.has_working_cards = this.any_visible_working();
            this.groups = build_display_groups(&this.cards);
            this.counts = counts;
            cx.notify();
        }));
        subs.push(cx.observe(&ui, |_, ui, cx| {
            let ms = ui.read(cx).agent_stale_threshold_ms();
            AgentRegistry::global(cx).update(cx, |reg, cx| reg.set_stale_threshold_ms(ms, cx));
        }));

        let refresh_task = cx.spawn(async move |this, cx| {
            let mut stale_elapsed = Duration::ZERO;
            let mut relative_elapsed = Duration::ZERO;
            loop {
                cx.background_executor().timer(ACTIVE_CARD_TICK).await;
                stale_elapsed += ACTIVE_CARD_TICK;
                relative_elapsed += ACTIVE_CARD_TICK;

                let alive = this
                    .update(cx, |this, cx| {
                        if stale_elapsed >= STALE_TICK {
                            stale_elapsed = Duration::ZERO;
                            if let Some(reg) = AgentRegistry::try_global(cx) {
                                reg.update(cx, |reg, cx| reg.refresh_stale(cx));
                            }
                        }

                        // Working cards need the animation cadence. Inactive cards
                        // need only the coarser relative-time refresh cadence.
                        let refresh_relative = relative_elapsed >= RELATIVE_TIME_TICK;
                        if refresh_relative {
                            relative_elapsed = Duration::ZERO;
                        }
                        if this.has_working_cards || refresh_relative {
                            cx.notify();
                        }
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
            cards,
            groups,
            counts,
            has_working_cards,
            _subs: subs,
            _refresh_task: refresh_task,
        }
    }

    /// Helper to create an `Entity<Self>`.
    pub fn new_entity(window: &mut gpui::Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn passes_filter(&self, card: &AgentCard) -> bool {
        card_passes_filter(self.filter, card)
    }

    /// Whether any visible card still needs the spinner cadence.
    fn any_visible_working(&self) -> bool {
        self.cards
            .iter()
            .any(|card| self.passes_filter(card) && card_is_working(card))
    }

    /// Number of ended cards in the registry snapshot (the "Clear ended" affordance).
    fn ended_count(&self) -> usize {
        self.cards
            .iter()
            .filter(|card| matches!(card.lifecycle, Lifecycle::Ended { .. }))
            .count()
    }
}

/// Filter rule shared by the list and the group badges (CORR-41).
///
/// An ended card counts as done — `AgentRegistry::summary` folds `Ended` into
/// `done` — so it stays visible (dimmed) under `All` and `Done`, and the Done
/// chip never advertises cards the filter cannot show. Its last `state` is
/// history, so it matches no other chip.
fn card_passes_filter(filter: Filter, card: &AgentCard) -> bool {
    if matches!(card.lifecycle, Lifecycle::Ended { .. }) {
        return matches!(filter, Filter::All | Filter::Done);
    }
    match filter {
        Filter::All => true,
        Filter::Working => card.state == AgentState::Working,
        Filter::Blocked => card.state == AgentState::Blocked,
        Filter::Errors => card.state == AgentState::Error,
        Filter::Idle => card.state == AgentState::Idle,
        Filter::Done => card.state == AgentState::Done,
    }
}

/// A live/stale card in the `Working` state animates; an ended one never does.
fn card_is_working(card: &AgentCard) -> bool {
    card.state == AgentState::Working && !matches!(card.lifecycle, Lifecycle::Ended { .. })
}

impl Focusable for AgentListView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::EventEmitter<PanelEvent> for AgentListView {}

/// The view *is* the right-dock Agent panel: it renders its own header +
/// scrolling card column full-bleed, so no wrapper panel is needed. Registered
/// as a raw `DockItem::Panel` (no tab bar / close / zoom chrome); the default
/// `dump` already records it as `PanelInfo::Panel`.
impl Panel for AgentListView {
    fn panel_name(&self) -> &'static str {
        panel_names::AGENT
    }

    fn title(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Agent"
    }

    fn closable(&self, _: &App) -> bool {
        // The panel is the whole right dock; closing is the dock's own toggle.
        false
    }

    fn zoomable(&self, _: &App) -> Option<PanelControl> {
        // Zoom is a TabPanel feature; `DockItem::Panel` is not subscribed to
        // zoom events by the library (see `DockArea::subscribe_item`).
        None
    }
}

#[derive(Clone, Copy)]
struct StatusChipSpec {
    filter: Filter,
    marker: &'static str,
    label: &'static str,
    count: usize,
    color: gpui::Hsla,
}

impl AgentListView {
    fn render_header(
        &self,
        counts: &AgentStateCounts,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ended = self.ended_count();
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
            .child(div().flex_1())
            // Ended cards are kept for review but never pruned on their own
            // (CORR-70); this is the one place that drops them.
            .when(ended > 0, |this| {
                this.child(
                    div()
                        .id("agent-clear-ended")
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(pal.muted)
                        .hover(|this| this.bg(pal.muted.opacity(0.12)))
                        .on_click(cx.listener(|_, _, _, cx| {
                            AgentRegistry::global(cx).update(cx, |reg, cx| reg.clear_ended(cx));
                        }))
                        .child(format!("Clear ended ({ended})")),
                )
            });

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
            .text_color(pal.foreground)
            .when(active, |this| this.bg(chip.color.opacity(0.18)))
            .hover(|this| this.bg(chip.color.opacity(0.12)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.filter = chip.filter;
                this.has_working_cards = this.any_visible_working();
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
        group: &AgentDisplayGroup,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let group_id = SharedString::from(format!("agent-group-{:?}", group.tab_key));
        let visible = || {
            group
                .card_indices
                .iter()
                .filter_map(|&index| self.cards.get(index))
                .filter(|card| self.passes_filter(card))
        };

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
                    .child(group.tab_title.clone()),
            )
            .children(group_badges(visible(), pal));

        let mut col = v_flex().w_full().gap_1().child(header);
        for card in visible() {
            col = col.child(self.render_card(card, pal, cx));
        }
        col
    }
}

impl Render for AgentListView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::capture(cx);
        // The observer refreshes this snapshot only when the registry changes.
        // Animation ticks therefore avoid cloning and summarizing the registry.
        let cards = &self.cards;
        let counts = self.counts;

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

        let mut list = v_flex()
            .id("agent-scroll")
            .w_full()
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .gap_2();
        for group in &self.groups {
            let has_visible_cards = group
                .card_indices
                .iter()
                .filter_map(|&index| cards.get(index))
                .any(|card| self.passes_filter(card));
            if has_visible_cards {
                list = list.child(self.render_group(group, &pal, cx));
            }
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

fn group_badges<'a>(cards: impl Iterator<Item = &'a AgentCard>, pal: &Palette) -> Vec<AnyElement> {
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
    let mut out: Vec<AnyElement> = Vec::new();
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

#[cfg(test)]
mod tests {
    use oneterm_state::Grouping;

    use super::*;

    fn card(state: AgentState, lifecycle: Lifecycle) -> AgentCard {
        let mut card = AgentCard::new(
            gpui::EntityId::from(1u64),
            "a".into(),
            &Grouping {
                tab_key: gpui::EntityId::from(2u64),
                tab_title: "tab".into(),
                space_number: 0,
                space_order: 0,
            },
        );
        card.state = state;
        card.lifecycle = lifecycle;
        card
    }

    /// CORR-41: ended cards are counted as done by the registry, so the Done
    /// and All chips must be able to show them; no other chip may.
    #[test]
    fn ended_cards_show_under_all_and_done_only() {
        let ended = card(AgentState::Working, Lifecycle::Ended { exit_code: None });
        assert!(card_passes_filter(Filter::All, &ended));
        assert!(card_passes_filter(Filter::Done, &ended));
        assert!(!card_passes_filter(Filter::Working, &ended));
        assert!(!card_passes_filter(Filter::Idle, &ended));

        let working = card(AgentState::Working, Lifecycle::Live);
        assert!(card_passes_filter(Filter::Working, &working));
        assert!(!card_passes_filter(Filter::Done, &working));
    }

    /// An ended card never drives the spinner cadence, whatever its last state.
    #[test]
    fn ended_cards_never_animate() {
        assert!(card_is_working(&card(AgentState::Working, Lifecycle::Live)));
        assert!(card_is_working(&card(
            AgentState::Working,
            Lifecycle::Stale
        )));
        assert!(!card_is_working(&card(
            AgentState::Working,
            Lifecycle::Ended { exit_code: Some(0) }
        )));
        assert!(!card_is_working(&card(AgentState::Idle, Lifecycle::Live)));
    }
}
