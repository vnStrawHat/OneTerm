use gpui::{
    AnyElement, Context, EntityId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{Icon, IconName, Sizable as _, h_flex, v_flex};

use oneterm_state::{AgentCard, AgentRegistry, AgentStateCounts, Lifecycle};
use oneterm_terminal::AgentState;

use crate::{AgentListView, Filter, card::Palette};

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

impl Render for AgentListView {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::capture(cx);
        let registry = AgentRegistry::global(cx);
        let (cards, counts) = {
            let reg = registry.read(cx);
            (reg.cards().to_vec(), reg.summary())
        };

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

fn group_badges(cards: &[AgentCard], pal: &Palette) -> Vec<AnyElement> {
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
