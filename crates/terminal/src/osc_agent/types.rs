//! Typed payload structs for OSC 9;7 agent-status events (spec §4.2).
//!
//! Each `type` maps to one struct here. All structs use `#[serde(default)]`
//! on optional fields and intentionally do **not** set `deny_unknown_fields`
//! (forward compatibility — spec §4.1 / §4.2: unknown fields are ignored).

use serde::Deserialize;

/// Agent lifecycle state (spec §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    /// Actively processing (LLM streaming, tool running).
    Working,
    /// Waiting for user input (approval/permission/prompt).
    Blocked,
    /// Turn finished, awaiting next prompt.
    Idle,
    /// Session ended cleanly.
    Done,
    /// Non-retryable error or crash.
    Error,
}

impl AgentState {
    /// Suggested host badge emoji (spec §5.1).
    pub fn badge(self) -> &'static str {
        match self {
            AgentState::Working => "🟢",
            AgentState::Blocked => "🟠",
            AgentState::Idle => "⚪",
            AgentState::Done => "✅",
            AgentState::Error => "❌",
        }
    }
}

/// Tool-call phase (spec §4.2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallPhase {
    Start,
    Update,
    End,
}

/// File action (spec §4.2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileAction {
    Read,
    Edit,
    Write,
    Delete,
    Move,
    Create,
}

/// Approval kind (spec §4.2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalKind {
    Permission,
    Confirm,
    Prompt,
    Select,
    /// Outcome of a previously-asked approval (spec §4.2.7 closing note).
    Resolved,
}

/// Risk level for an approval (spec §4.2.7 `risk`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalRisk {
    Low,
    Medium,
    High,
}

/// Model change source (spec §4.2.4 `source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    Set,
    Cycle,
    Restore,
}

/// Rich option for an approval `select` (spec §4.2.7 `choices`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApprovalChoice {
    pub value: String,
    pub label: String,
}

/// `type: "state"` — agent lifecycle state (spec §4.2.1).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StateEvent {
    pub state: AgentState,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `type: "session"` — session identity (spec §4.2.2).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionIdentityEvent {
    pub session_id: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// `type: "heartbeat"` — keepalive (spec §4.2.3).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HeartbeatEvent {
    #[serde(default)]
    pub interval_ms: Option<u64>,
    #[serde(default)]
    pub state: Option<AgentState>,
}

/// `type: "model"` — active model (spec §4.2.4).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModelEvent {
    pub provider: String,
    pub model_id: String,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub source: Option<ModelSource>,
    #[serde(default)]
    pub context_used: Option<u64>,
}

/// `type: "tool_call"` — tool invocation (spec §4.2.5).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ToolCallEvent {
    pub tool_call_id: String,
    pub tool: String,
    pub phase: ToolCallPhase,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub args_redacted: Option<bool>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub is_error: Option<bool>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub diff_stat: Option<String>,
    #[serde(default)]
    pub progress: Option<String>,
}

/// `type: "file"` — file activity (spec §4.2.6).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileEvent {
    pub path: String,
    pub action: FileAction,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub dest: Option<String>,
}

/// `type: "approval"` — structured approval request (spec §4.2.7).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApprovalEvent {
    pub id: String,
    pub kind: ApprovalKind,
    pub prompt: String,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub risk: Option<ApprovalRisk>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub choices: Option<Vec<ApprovalChoice>>,
}
