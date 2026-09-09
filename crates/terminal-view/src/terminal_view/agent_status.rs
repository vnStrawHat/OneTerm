//! OSC 9;7 agent status → the Agent Panel model (`oneterm_state::AgentRegistry`).
//!
//! The view tags every event with its Tab/Space grouping, registers the
//! navigation target that focuses this terminal, and reports process death;
//! a true close (`shutdown`) removes the cards instead.

use std::sync::Arc;

use gpui::Context;
use oneterm_state::{Grouping, Lifecycle};
use oneterm_terminal::AgentStatusEvent;

use super::TerminalView;

impl TerminalView {
    /// Fold an OSC 9;7 event into the registry and refresh this terminal's
    /// navigation entry. No-op until the registry is available and the panel
    /// has wired up `split_ctx` (always set for a live terminal leaf).
    pub(super) fn push_agent_status(&self, ev: &Arc<AgentStatusEvent>, cx: &mut Context<Self>) {
        log::debug!(
            "push_agent_status: recv agent={} type={} seq={}",
            ev.agent(),
            ev.type_name(),
            ev.seq()
        );
        let Some(registry) = self.deps.agent_registry.clone() else {
            log::debug!("push_agent_status: AgentRegistry not available — dropping");
            return;
        };
        let Some(sc) = self.split_ctx.clone() else {
            log::debug!("push_agent_status: no split_ctx — dropping");
            return;
        };
        let Some(panel) = sc.panel.upgrade() else {
            log::debug!("push_agent_status: split_ctx.panel already dropped — dropping");
            return;
        };
        let terminal_key = cx.entity_id();
        let tab_key = panel.entity_id();

        let (grouping, nav) = {
            let p = panel.read(cx);
            // The live OSC 0/2 title comes from OUR OWN session (a different
            // entity — safe to read while the view is leased). `p.tab_label(cx)`
            // would re-read the active view, which is this view mid-`update`
            // — a double lease.
            let live_title = self.session.read(cx).title();
            let grouping = Grouping {
                tab_key,
                tab_title: p.tab_label_with_title(live_title.as_deref(), cx),
                space_number: sc.space_id.display_number(),
                space_order: p.space_order(sc.space_id),
            };
            let nav = crate::agent::agent_nav(p.tab_panel_weak(), sc.panel.clone(), sc.space_id);
            (grouping, nav)
        };

        registry.update(cx, |reg, cx| {
            reg.set_nav(terminal_key, nav);
            reg.apply(terminal_key, grouping, ev, cx);
        });
    }

    /// Mark this terminal's agent card(s) as `Ended` (the host is authoritative
    /// for process death). No-op without a registry or when the terminal never
    /// reported an agent.
    pub(super) fn mark_agent_ended(&self, exit_code: Option<i32>, cx: &mut Context<Self>) {
        if let Some(registry) = self.deps.agent_registry.clone() {
            let key = cx.entity_id();
            registry.update(cx, |reg, cx| {
                reg.set_lifecycle(key, Lifecycle::Ended { exit_code }, cx);
            });
        }
    }
}
