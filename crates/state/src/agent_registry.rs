//! `AgentRegistry` — the aggregated, folded display model behind the Agent Panel.
//!
//! OSC 9;7 events arrive as a **stream** of small, single-`type` messages
//! (`state`, `session`, `heartbeat`, `model`, `tool_call`, `file`, `approval` —
//! see `docs/osc-agent-status.md` §4.2). A useful panel needs the *folded*
//! history of that stream per agent: the current lifecycle state **and** the
//! active model + context usage, the running / last tool call, a recent-file
//! feed, and any pending approval. This module holds that fold.
//!
//! Design: [`docs/agent-panel-display.md`] §3. The registry is a global
//! `Entity<AgentRegistry>` (registered like [`crate::AppState`]). It lives in
//! `oneterm-state` — below both the feature UI crates and the shell — so the
//! Agent Panel stays feature-agnostic: `terminal-view` *feeds* the registry
//! (pushing events + grouping + lifecycle) and `agent-ui` *reads* it.
//!
//! `seq` dedup is already applied upstream (backend listeners, see
//! `oneterm_terminal::osc_agent::should_apply`); the registry keeps a
//! `last_seq` guard purely for defensiveness.

use std::collections::VecDeque;
use std::time::Instant;

use gpui::{App, AppContext, Context, Entity, EntityId, Global};

use oneterm_terminal::{
    AgentState, AgentStatusEvent, ApprovalChoice, ApprovalEvent, ApprovalKind, ApprovalRisk,
    FileAction, ModelEvent, ToolCallEvent, ToolCallPhase,
};

// ── Display-side truncation caps (chars) ────────────────────────────────
// The parser already enforces the 8 KiB envelope cap (spec §3.2); these bound
// each free-text field to a sensible single-line length for display.
const CAP_MESSAGE: usize = 256;
const CAP_PROMPT: usize = 1024;
const CAP_ARGS: usize = 200;
const CAP_PATH: usize = 256;
const CAP_ID: usize = 64;
const CAP_PROGRESS: usize = 120;
const CAP_ERROR: usize = 256;
const CAP_TARGET: usize = 160;
const CAP_DIFF: usize = 32;
const CAP_SHORT: usize = 64;
const CAP_PROVIDER: usize = 48;
const CAP_MODEL: usize = 96;

const RECENT_TOOLS_CAP: usize = 8;
const RECENT_FILES_CAP: usize = 6;

/// Sanitize an untrusted free-text field for display (spec §10 / §4.3):
/// strip control characters (so it can never re-enter a VT path), collapse to a
/// single line, trim, and truncate to `max` chars with an ellipsis.
fn sanitize_line(s: &str, max: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().count() > max {
        let mut out: String = trimmed.chars().take(max).collect();
        out.push('…');
        out
    } else {
        trimmed.to_string()
    }
}

fn sanitize_opt(s: &Option<String>, max: usize) -> Option<String> {
    s.as_deref().map(|v| sanitize_line(v, max))
}

// ── Lifecycle / liveness ────────────────────────────────────────────────

/// Host-authoritative lifecycle of an agent card (spec §5.2.7 / §5.3).
///
/// Independent of the agent-reported [`AgentState`]: `Ended` is set by the host
/// on terminal process death, `Stale` by the registry's periodic tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// A recent event arrived within the stale window.
    Live,
    /// No OSC 9;7 for the stale window while the process is alive.
    Stale,
    /// The terminal process exited (host-authoritative).
    Ended { exit_code: Option<i32> },
}

// ── Folded sub-structures ───────────────────────────────────────────────

/// Active model + context info (from `model` events, spec §4.2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub provider: String,
    pub model_id: String,
    pub model_name: Option<String>,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub reasoning: bool,
}

impl ModelInfo {
    fn from_event(ev: &ModelEvent) -> Self {
        Self {
            provider: sanitize_line(&ev.provider, CAP_PROVIDER),
            model_id: sanitize_line(&ev.model_id, CAP_MODEL),
            model_name: sanitize_opt(&ev.model_name, CAP_MODEL),
            context_window: ev.context_window,
            max_output_tokens: ev.max_output_tokens,
            reasoning: ev.reasoning.unwrap_or(false),
        }
    }

    /// The label to show: `model_name` if present, else `model_id`.
    pub fn display_name(&self) -> &str {
        self.model_name.as_deref().unwrap_or(&self.model_id)
    }
}

/// A single tool invocation (from `tool_call` events, spec §4.2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRun {
    pub id: String,
    pub tool: String,
    pub target: Option<String>,
    pub args: Option<String>,
    pub args_redacted: bool,
    pub progress: Option<String>,
    /// Agent-clock ms at `start`.
    pub started_ts: u64,
    pub ended: bool,
    pub exit_code: Option<i64>,
    pub is_error: bool,
    pub error_message: Option<String>,
    pub duration_ms: Option<u64>,
    pub diff_stat: Option<String>,
}

impl ToolRun {
    fn from_start(ev: &ToolCallEvent, ts: u64) -> Self {
        Self {
            id: sanitize_line(&ev.tool_call_id, CAP_ID),
            tool: sanitize_line(&ev.tool, CAP_SHORT),
            target: sanitize_opt(&ev.target, CAP_TARGET),
            args: sanitize_opt(&ev.args, CAP_ARGS),
            args_redacted: ev.args_redacted.unwrap_or(false),
            progress: sanitize_opt(&ev.progress, CAP_PROGRESS),
            started_ts: ts,
            ended: false,
            exit_code: None,
            is_error: false,
            error_message: None,
            duration_ms: None,
            diff_stat: None,
        }
    }

    fn finalize(&mut self, ev: &ToolCallEvent) {
        self.ended = true;
        self.exit_code = ev.exit_code;
        self.is_error = ev.is_error.unwrap_or(false);
        self.error_message = sanitize_opt(&ev.error_message, CAP_ERROR);
        self.duration_ms = ev.duration_ms;
        self.diff_stat = sanitize_opt(&ev.diff_stat, CAP_DIFF);
        // Enrich fields that may only appear on the end event.
        if self.target.is_none() {
            self.target = sanitize_opt(&ev.target, CAP_TARGET);
        }
        if self.args.is_none() {
            self.args = sanitize_opt(&ev.args, CAP_ARGS);
            self.args_redacted = ev.args_redacted.unwrap_or(self.args_redacted);
        }
    }
}

/// A file-activity entry (from `file` events, spec §4.2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub action: FileAction,
    pub dest: Option<String>,
    pub tool_call_id: Option<String>,
}

/// A pending approval request (from `approval` events, spec §4.2.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalInfo {
    pub id: String,
    pub kind: ApprovalKind,
    pub prompt: String,
    pub options: Option<Vec<String>>,
    pub default: Option<String>,
    pub tool: Option<String>,
    pub risk: Option<ApprovalRisk>,
    pub timeout_ms: Option<u64>,
    pub choices: Option<Vec<ApprovalChoice>>,
}

impl ApprovalInfo {
    fn from_event(ev: &ApprovalEvent) -> Self {
        let options = ev.options.as_ref().map(|opts| {
            opts.iter()
                .map(|o| sanitize_line(o, CAP_SHORT))
                .collect::<Vec<_>>()
        });
        let choices = ev.choices.as_ref().map(|cs| {
            cs.iter()
                .map(|c| ApprovalChoice {
                    value: sanitize_line(&c.value, CAP_SHORT),
                    label: sanitize_line(&c.label, CAP_SHORT),
                })
                .collect::<Vec<_>>()
        });
        Self {
            id: sanitize_line(&ev.id, CAP_ID),
            kind: ev.kind,
            prompt: sanitize_line(&ev.prompt, CAP_PROMPT),
            options,
            default: sanitize_opt(&ev.default, CAP_SHORT),
            tool: sanitize_opt(&ev.tool, CAP_SHORT),
            risk: ev.risk,
            timeout_ms: ev.timeout_ms,
            choices,
        }
    }
}

// ── Grouping metadata (pushed by terminal-view) ─────────────────────────

/// Grouping / labelling metadata for a card, supplied by `terminal-view`
/// (the only crate that knows the Tab/Space hierarchy) alongside each event.
/// This keeps `state` feature-agnostic — it never depends on `terminal-view`.
#[derive(Debug, Clone)]
pub struct Grouping {
    /// `EntityId` of the `TerminalPanel` (the Tab) — the group key.
    pub tab_key: EntityId,
    /// Tab title (resolved OSC 0/2 title or fallback).
    pub tab_title: String,
    /// `single` for a one-Space tab, else `#N` (SpaceTree order).
    pub space_label: String,
    /// Whether this agent's Space is the focused one.
    pub space_active: bool,
}

// ── The folded per-agent card ───────────────────────────────────────────

/// The folded state of one agent, keyed by `(terminal_key, agent_id)`.
#[derive(Debug, Clone)]
pub struct AgentCard {
    // ── identity / grouping ──
    pub terminal_key: EntityId,
    pub agent_id: String,
    pub tab_key: EntityId,
    pub tab_title: String,
    pub space_label: String,
    pub space_active: bool,

    // ── lifecycle (from `state` / `session`) ──
    pub state: AgentState,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub session_reason: Option<String>,
    pub parent_id: Option<String>,

    // ── liveness ──
    pub last_event_ts: u64,
    pub last_recv: Instant,
    pub heartbeat_interval: Option<u64>,
    pub lifecycle: Lifecycle,

    // ── model / context ──
    pub model: Option<ModelInfo>,
    pub context_used: Option<u64>,

    // ── activity ──
    pub current_tool: Option<ToolRun>,
    pub recent_tools: VecDeque<ToolRun>,

    // ── files ──
    pub recent_files: VecDeque<FileEntry>,

    // ── approval ──
    pub pending_approval: Option<ApprovalInfo>,
    pub resolved_note: Option<String>,

    // ── debug ──
    pub last_seq: u64,
}

impl AgentCard {
    fn new(terminal_key: EntityId, agent_id: String, grouping: &Grouping) -> Self {
        Self {
            terminal_key,
            agent_id,
            tab_key: grouping.tab_key,
            tab_title: grouping.tab_title.clone(),
            space_label: grouping.space_label.clone(),
            space_active: grouping.space_active,
            state: AgentState::Idle,
            message: None,
            session_id: None,
            session_reason: None,
            parent_id: None,
            last_event_ts: 0,
            last_recv: Instant::now(),
            heartbeat_interval: None,
            lifecycle: Lifecycle::Live,
            model: None,
            context_used: None,
            current_tool: None,
            recent_tools: VecDeque::new(),
            recent_files: VecDeque::new(),
            pending_approval: None,
            resolved_note: None,
            last_seq: 0,
        }
    }

    /// Fold one event into this card (spec §3.3). `seq` dedup is already applied
    /// upstream; the `last_seq` guard is defensive only.
    fn apply_event(&mut self, ev: &AgentStatusEvent) {
        self.last_seq = ev.seq();
        self.last_event_ts = ev.ts();
        self.last_recv = Instant::now();
        // A fresh event un-stales the card, unless the host already declared it
        // ended (process death is authoritative).
        if !matches!(self.lifecycle, Lifecycle::Ended { .. }) {
            self.lifecycle = Lifecycle::Live;
        }

        match ev {
            AgentStatusEvent::State { payload, .. } => {
                self.state = payload.state;
                self.message = sanitize_opt(&payload.message, CAP_MESSAGE);
                if let Some(sid) = &payload.session_id {
                    self.session_id = Some(sanitize_line(sid, CAP_ID));
                }
                if payload.state != AgentState::Blocked {
                    self.pending_approval = None;
                }
                if payload.state == AgentState::Done
                    && !matches!(self.lifecycle, Lifecycle::Ended { .. })
                {
                    self.lifecycle = Lifecycle::Ended { exit_code: None };
                }
            }
            AgentStatusEvent::Session { payload, .. } => {
                self.session_id = Some(sanitize_line(&payload.session_id, CAP_ID));
                self.session_reason = sanitize_opt(&payload.reason, CAP_SHORT);
                self.parent_id = sanitize_opt(&payload.parent_id, CAP_ID);
            }
            AgentStatusEvent::Heartbeat { payload, .. } => {
                self.heartbeat_interval = payload.interval_ms;
                if let Some(st) = payload.state {
                    self.state = st;
                }
            }
            AgentStatusEvent::Model { payload, .. } => {
                self.model = Some(ModelInfo::from_event(payload));
                if let Some(used) = payload.context_used {
                    self.context_used = Some(used);
                }
            }
            AgentStatusEvent::ToolCall { payload, .. } => self.apply_tool_call(payload, ev.ts()),
            AgentStatusEvent::File { payload, .. } => {
                let entry = FileEntry {
                    path: sanitize_line(&payload.path, CAP_PATH),
                    action: payload.action,
                    dest: sanitize_opt(&payload.dest, CAP_PATH),
                    tool_call_id: sanitize_opt(&payload.tool_call_id, CAP_ID),
                };
                // Dedup consecutive identical entries.
                if self.recent_files.back() != Some(&entry) {
                    push_capped(&mut self.recent_files, entry, RECENT_FILES_CAP);
                }
            }
            AgentStatusEvent::Approval { payload, .. } => match payload.kind {
                ApprovalKind::Resolved => {
                    let note = payload
                        .default
                        .as_deref()
                        .map(|d| sanitize_line(d, CAP_SHORT))
                        .unwrap_or_else(|| "resolved".to_string());
                    self.resolved_note = Some(note);
                    self.pending_approval = None;
                }
                _ => {
                    self.pending_approval = Some(ApprovalInfo::from_event(payload));
                    self.resolved_note = None;
                }
            },
        }
    }

    fn apply_tool_call(&mut self, payload: &ToolCallEvent, ts: u64) {
        match payload.phase {
            ToolCallPhase::Start => {
                self.current_tool = Some(ToolRun::from_start(payload, ts));
            }
            ToolCallPhase::Update => {
                if let Some(ct) = &mut self.current_tool {
                    if ct.id == sanitize_line(&payload.tool_call_id, CAP_ID) {
                        if let Some(p) = &payload.progress {
                            ct.progress = Some(sanitize_line(p, CAP_PROGRESS));
                        }
                    }
                }
            }
            ToolCallPhase::End => {
                let want_id = sanitize_line(&payload.tool_call_id, CAP_ID);
                let mut run = match self.current_tool.take() {
                    Some(ct) if ct.id == want_id => ct,
                    Some(other) => {
                        // End event for a different id than the running tool:
                        // keep the running one, synthesize a run for the end.
                        self.current_tool = Some(other);
                        ToolRun::from_start(payload, ts)
                    }
                    None => ToolRun::from_start(payload, ts),
                };
                run.finalize(payload);
                push_capped(&mut self.recent_tools, run, RECENT_TOOLS_CAP);
            }
        }
    }

    /// Age in seconds since the last received event (host clock).
    pub fn age_secs(&self) -> u64 {
        self.last_recv.elapsed().as_secs()
    }

    /// Sort rank for card ordering within a tab group (spec §4.2):
    /// blocked → error → working → stale → idle → done → ended.
    pub fn sort_rank(&self) -> u8 {
        if matches!(self.lifecycle, Lifecycle::Ended { .. }) {
            return 6;
        }
        match self.state {
            AgentState::Blocked => 0,
            AgentState::Error => 1,
            AgentState::Working => 2,
            _ if self.lifecycle == Lifecycle::Stale => 3,
            AgentState::Idle => 4,
            AgentState::Done => 5,
        }
    }

    /// Whether this card is in a resting state (idle or done/ended) — used by the
    /// "collapse idle/done" group behaviour.
    pub fn is_resting(&self) -> bool {
        matches!(self.lifecycle, Lifecycle::Ended { .. })
            || matches!(self.state, AgentState::Idle | AgentState::Done)
    }
}

/// Push `value` to the back of `buf`, dropping the front if over `cap`.
fn push_capped<T>(buf: &mut VecDeque<T>, value: T, cap: usize) {
    buf.push_back(value);
    while buf.len() > cap {
        buf.pop_front();
    }
}

// ── Summary counts (for the header) ─────────────────────────────────────

/// Aggregate state counts across all cards (spec §8 summary line).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentStateCounts {
    pub working: usize,
    pub blocked: usize,
    pub idle: usize,
    pub done: usize,
    pub error: usize,
    pub total: usize,
}

// ── The registry ────────────────────────────────────────────────────────

/// Global folded model behind the Agent Panel.
///
/// Cards are stored insertion-ordered (a card appears in the order its agent
/// first reported); grouping / ordering / filtering for display is applied by
/// the `agent-ui` view layer.
pub struct AgentRegistry {
    cards: Vec<AgentCard>,
    stale_threshold_ms: u64,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self {
            cards: Vec::new(),
            stale_threshold_ms: 300_000,
        }
    }
}

/// Global wrapper for `Entity<AgentRegistry>`.
pub struct AgentRegistryGlobal(pub Entity<AgentRegistry>);

impl Global for AgentRegistryGlobal {}

impl AgentRegistry {
    /// Get the global `Entity<AgentRegistry>` (panics if not initialized).
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<AgentRegistryGlobal>().0.clone()
    }

    /// Get the global `Entity<AgentRegistry>` if initialized.
    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<AgentRegistryGlobal>().map(|g| g.0.clone())
    }

    /// Initialize the global registry.
    pub fn init(cx: &mut App) {
        if cx.try_global::<AgentRegistryGlobal>().is_none() {
            let entity = cx.new(|_| Self::default());
            cx.set_global(AgentRegistryGlobal(entity));
        }
    }

    /// All cards, insertion-ordered.
    pub fn cards(&self) -> &[AgentCard] {
        &self.cards
    }

    /// Whether there are no cards.
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// The effective staleness threshold (ms).
    pub fn stale_threshold_ms(&self) -> u64 {
        self.stale_threshold_ms
    }

    /// Update the staleness threshold (from `UiConfig`). Notifies on change.
    pub fn set_stale_threshold_ms(&mut self, ms: u64, cx: &mut Context<Self>) {
        if self.stale_threshold_ms != ms {
            self.stale_threshold_ms = ms;
            cx.notify();
        }
    }

    /// Aggregate state counts across all cards (Ended counts as done).
    pub fn summary(&self) -> AgentStateCounts {
        let mut c = AgentStateCounts::default();
        for card in &self.cards {
            c.total += 1;
            if matches!(card.lifecycle, Lifecycle::Ended { .. }) {
                c.done += 1;
                continue;
            }
            match card.state {
                AgentState::Working => c.working += 1,
                AgentState::Blocked => c.blocked += 1,
                AgentState::Idle => c.idle += 1,
                AgentState::Done => c.done += 1,
                AgentState::Error => c.error += 1,
            }
        }
        c
    }

    /// Update the visible tab title for every card in a tab group.
    pub fn rename_tab_title(
        &mut self,
        tab_key: EntityId,
        tab_title: String,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.tab_key != tab_key || card.tab_title == tab_title {
                continue;
            }
            card.tab_title = tab_title.clone();
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Fold one event into the matching card (create if absent), refreshing its
    /// grouping metadata. `seq` dedup is already applied upstream.
    pub fn apply(
        &mut self,
        terminal_key: EntityId,
        grouping: Grouping,
        ev: &AgentStatusEvent,
        cx: &mut Context<Self>,
    ) {
        let agent_id = ev.agent();
        let idx = match self
            .cards
            .iter()
            .position(|c| c.terminal_key == terminal_key && c.agent_id == agent_id)
        {
            Some(i) => i,
            None => {
                self.cards.push(AgentCard::new(
                    terminal_key,
                    agent_id.to_string(),
                    &grouping,
                ));
                self.cards.len() - 1
            }
        };
        let card = &mut self.cards[idx];
        card.tab_key = grouping.tab_key;
        card.tab_title = grouping.tab_title;
        card.space_label = grouping.space_label;
        card.space_active = grouping.space_active;
        card.apply_event(ev);
        cx.notify();
    }

    /// Set the lifecycle of every card belonging to `terminal_key` (the host
    /// reports terminal exit/close — spec §5.2.7). Notifies on change.
    pub fn set_lifecycle(
        &mut self,
        terminal_key: EntityId,
        lifecycle: Lifecycle,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.terminal_key != terminal_key {
                continue;
            }
            // A later `Closed` (Ended{None}) must not clobber an exit code that a
            // preceding `Exited(code)` already recorded.
            let next = match (card.lifecycle, lifecycle) {
                (
                    Lifecycle::Ended {
                        exit_code: Some(existing),
                    },
                    Lifecycle::Ended { exit_code: None },
                ) => Lifecycle::Ended {
                    exit_code: Some(existing),
                },
                _ => lifecycle,
            };
            if card.lifecycle != next {
                card.lifecycle = next;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Drop every card for a terminal that was closed / dragged away.
    pub fn remove_terminal(&mut self, terminal_key: EntityId, cx: &mut Context<Self>) {
        let before = self.cards.len();
        self.cards.retain(|c| c.terminal_key != terminal_key);
        if self.cards.len() != before {
            cx.notify();
        }
    }

    /// Remove all ended cards (the ⚙ "Clear ended" action).
    pub fn clear_ended(&mut self, cx: &mut Context<Self>) {
        let before = self.cards.len();
        self.cards
            .retain(|c| !matches!(c.lifecycle, Lifecycle::Ended { .. }));
        if self.cards.len() != before {
            cx.notify();
        }
    }

    /// Low-frequency tick: mark `Live` cards `Stale` when no event has arrived
    /// within `max(stale_threshold, 3 × heartbeat_interval)` (spec §5.3). A
    /// threshold of `0` disables staleness marking.
    pub fn refresh_stale(&mut self, cx: &mut Context<Self>) {
        if self.stale_threshold_ms == 0 {
            return;
        }
        let mut changed = false;
        for card in self.cards.iter_mut() {
            if card.lifecycle != Lifecycle::Live {
                continue;
            }
            let hb = card
                .heartbeat_interval
                .map(|i| i.saturating_mul(3))
                .unwrap_or(0);
            let effective_ms = self.stale_threshold_ms.max(hb);
            if card.last_recv.elapsed().as_millis() as u64 >= effective_ms {
                card.lifecycle = Lifecycle::Stale;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneterm_terminal::{
        ApprovalEvent, FileEvent, ModelEvent, StateEvent, ToolCallEvent, ToolCallPhase,
    };

    fn card() -> AgentCard {
        let key = EntityId::from(1u64);
        AgentCard::new(
            key,
            "pi".into(),
            &Grouping {
                tab_key: EntityId::from(2u64),
                tab_title: "t".into(),
                space_label: "single".into(),
                space_active: false,
            },
        )
    }

    fn state_ev(seq: u64, state: AgentState, msg: Option<&str>) -> AgentStatusEvent {
        AgentStatusEvent::State {
            agent: "pi".into(),
            seq,
            ts: seq * 1000,
            payload: StateEvent {
                state,
                message: msg.map(|s| s.to_string()),
                session_id: None,
            },
        }
    }

    #[test]
    fn sanitize_strips_control_and_truncates() {
        assert_eq!(sanitize_line("a\nb\tc", 100), "a b c");
        assert_eq!(sanitize_line("  hi  ", 100), "hi");
        let long: String = "x".repeat(300);
        let out = sanitize_line(&long, 10);
        assert_eq!(out.chars().count(), 11); // 10 + ellipsis
        assert!(out.ends_with('…'));
    }

    #[test]
    fn state_fold_sets_state_and_message() {
        let mut c = card();
        c.apply_event(&state_ev(1, AgentState::Working, Some("thinking")));
        assert_eq!(c.state, AgentState::Working);
        assert_eq!(c.message.as_deref(), Some("thinking"));
        assert_eq!(c.last_seq, 1);
    }

    #[test]
    fn done_state_marks_ended() {
        let mut c = card();
        c.apply_event(&state_ev(1, AgentState::Done, None));
        assert!(matches!(c.lifecycle, Lifecycle::Ended { exit_code: None }));
    }

    #[test]
    fn tool_call_start_update_end_folds_to_recent() {
        let mut c = card();
        c.apply_event(&AgentStatusEvent::ToolCall {
            agent: "pi".into(),
            seq: 1,
            ts: 1,
            payload: ToolCallEvent {
                tool_call_id: "t1".into(),
                tool: "bash".into(),
                phase: ToolCallPhase::Start,
                target: None,
                args: Some("ls".into()),
                args_redacted: None,
                exit_code: None,
                is_error: None,
                error_message: None,
                duration_ms: None,
                diff_stat: None,
                progress: None,
            },
        });
        assert!(c.current_tool.is_some());
        c.apply_event(&AgentStatusEvent::ToolCall {
            agent: "pi".into(),
            seq: 2,
            ts: 2,
            payload: ToolCallEvent {
                tool_call_id: "t1".into(),
                tool: "bash".into(),
                phase: ToolCallPhase::End,
                target: None,
                args: None,
                args_redacted: None,
                exit_code: Some(0),
                is_error: Some(false),
                error_message: None,
                duration_ms: Some(42),
                diff_stat: None,
                progress: None,
            },
        });
        assert!(c.current_tool.is_none());
        assert_eq!(c.recent_tools.len(), 1);
        assert_eq!(c.recent_tools[0].duration_ms, Some(42));
    }

    #[test]
    fn file_feed_dedups_consecutive() {
        let mut c = card();
        let mk = |seq| AgentStatusEvent::File {
            agent: "pi".into(),
            seq,
            ts: seq,
            payload: FileEvent {
                path: "src/app.rs".into(),
                action: FileAction::Edit,
                tool_call_id: None,
                dest: None,
            },
        };
        c.apply_event(&mk(1));
        c.apply_event(&mk(2));
        assert_eq!(c.recent_files.len(), 1);
    }

    #[test]
    fn approval_sets_and_resolves() {
        let mut c = card();
        c.apply_event(&AgentStatusEvent::Approval {
            agent: "pi".into(),
            seq: 1,
            ts: 1,
            payload: ApprovalEvent {
                id: "a1".into(),
                kind: ApprovalKind::Confirm,
                prompt: "ok?".into(),
                options: None,
                default: Some("no".into()),
                tool: None,
                tool_call_id: None,
                risk: Some(ApprovalRisk::High),
                timeout_ms: None,
                choices: None,
            },
        });
        assert!(c.pending_approval.is_some());
        // A non-blocked state clears the pending approval.
        c.apply_event(&state_ev(2, AgentState::Working, None));
        assert!(c.pending_approval.is_none());
    }

    #[test]
    fn model_fold_keeps_latest_context_used() {
        let mut c = card();
        let mk = |seq, used| AgentStatusEvent::Model {
            agent: "pi".into(),
            seq,
            ts: seq,
            payload: ModelEvent {
                provider: "anthropic".into(),
                model_id: "claude".into(),
                model_name: Some("Claude".into()),
                context_window: Some(200_000),
                max_output_tokens: None,
                reasoning: Some(true),
                source: None,
                context_used: Some(used),
            },
        };
        c.apply_event(&mk(1, 1000));
        c.apply_event(&mk(2, 2000));
        assert_eq!(c.context_used, Some(2000));
        assert_eq!(c.model.as_ref().unwrap().display_name(), "Claude");
    }
}
