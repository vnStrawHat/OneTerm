//! Folded agent-card data and OSC 9;7 event application.
//!
//! This module owns the backend-neutral display model. Global GPUI entity
//! registration and lifecycle management remain in `agent_registry`.

use std::collections::VecDeque;
use std::time::Instant;

use gpui::EntityId;

use oneterm_terminal::{
    AgentPayload, AgentState, AgentStatusEvent, ApprovalChoice, ApprovalEvent, ApprovalKind,
    ApprovalRisk, FileAction, ModelEvent, ToolCallEvent, ToolCallPhase,
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
    /// Stable, user-facing Space number supplied by `terminal-view`.
    pub space_number: u64,
    /// 0-based depth-first position used only for within-tab card ordering.
    pub space_order: usize,
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
    pub space_number: u64,
    pub space_order: usize,

    // ── lifecycle (from `state` / `session`) ──
    pub state: AgentState,
    pub message: Option<String>,
    pub session_id: Option<String>,
    pub session_reason: Option<String>,
    pub parent_id: Option<String>,
    pub project_dir: Option<String>,
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
    /// A fresh `Idle`/`Live` card for `agent_id` in the terminal `terminal_key`,
    /// placed by `grouping`; every other field starts empty.
    pub fn new(terminal_key: EntityId, agent_id: String, grouping: &Grouping) -> Self {
        Self {
            terminal_key,
            agent_id,
            tab_key: grouping.tab_key,
            tab_title: grouping.tab_title.clone(),
            space_number: grouping.space_number,
            space_order: grouping.space_order,
            state: AgentState::Idle,
            message: None,
            session_id: None,
            session_reason: None,
            parent_id: None,
            project_dir: None,
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
    pub(crate) fn apply_event(&mut self, ev: &AgentStatusEvent) {
        self.last_seq = ev.seq();
        self.last_event_ts = ev.ts();
        self.last_recv = Instant::now();
        // A fresh event un-stales the card, unless the host already declared it
        // ended (process death is authoritative).
        if !matches!(self.lifecycle, Lifecycle::Ended { .. }) {
            self.lifecycle = Lifecycle::Live;
        }

        match &ev.payload {
            AgentPayload::State(payload) => {
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
            AgentPayload::Session(payload) => {
                self.session_id = Some(sanitize_line(&payload.session_id, CAP_ID));
                self.session_reason = sanitize_opt(&payload.reason, CAP_SHORT);
                self.parent_id = sanitize_opt(&payload.parent_id, CAP_ID);
                self.project_dir = sanitize_opt(&payload.project_dir, CAP_PATH);
            }
            AgentPayload::Heartbeat(payload) => {
                self.heartbeat_interval = payload.interval_ms;
                if let Some(st) = payload.state {
                    self.state = st;
                }
            }
            AgentPayload::Model(payload) => {
                self.model = Some(ModelInfo::from_event(payload));
                if let Some(used) = payload.context_used {
                    self.context_used = Some(used);
                }
            }
            AgentPayload::ToolCall(payload) => self.apply_tool_call(payload, ev.ts()),
            AgentPayload::File(payload) => {
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
            AgentPayload::Approval(payload) => match payload.kind {
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

#[cfg(test)]
#[path = "agent_model_tests.rs"]
mod tests;
