//! OSC 9;7 — Agent Status Event receiver.
//!
//! Parses the agent side-channel described in `docs/osc-agent-status.md`:
//! an agent running inside the terminal emits
//!
//! ```text
//! ESC ] 9 ; 7 ; <base64-json> ST
//! ```
//!
//! to report its lifecycle state, session identity, model/context, tool
//! calls, file activity, and approval requests. The payload is **always**
//! base64-wrapped (see spec §3.1: VT engines split the OSC on `;` before
//! dispatch, so raw JSON cannot survive). The receiver:
//!
//! 1. takes the third parameter (`params[2]`) as the base64 blob,
//! 2. enforces an 8 KiB cap on the base64 length (spec §3.2),
//! 3. base64-decodes + UTF-8-decodes + JSON-parses it,
//! 4. validates the envelope (`v`, `agent`, `type`, `seq`, `ts`),
//! 5. dispatches on `type` into a typed [`AgentStatusEvent`].
//!
//! On any malformed input (bad base64, bad UTF-8, bad JSON, unknown schema
//! version, unknown `type`, missing required fields) the receiver drops
//! the event silently (spec §3.3) — a `log::debug!` is allowed.
//!
//! `seq` dedup is **not** done here — it is per-(terminal, agent) state and
//! belongs to the listener that owns the terminal's state cache. This module
//! only produces a parsed [`AgentStatusEvent`]; the listener decides whether
//! to apply it (see `docs/osc-agent-status.md` §4.1 `seq` and §8.3).
//!
//! Forward compatibility: unknown envelope fields and unknown per-type
//! fields are ignored (serde `deny_unknown_fields` is intentionally **not**
//! used).

mod dedup;
mod types;

pub use types::{
    AgentState, ApprovalChoice, ApprovalEvent, ApprovalKind, ApprovalRisk, FileAction, FileEvent,
    HeartbeatEvent, ModelEvent, ModelSource, SessionIdentityEvent, StateEvent, ToolCallEvent,
    ToolCallPhase,
};

pub use dedup::{AgentSeqWatermarks, MAX_TRACKED_AGENTS, should_apply};

use base64::Engine;
use serde::Deserialize;

/// Maximum base64 payload length accepted on OSC 9;7 (spec §3.2).
///
/// ~6 KiB of raw JSON after decode. Oversized payloads are dropped silently.
pub const MAX_AGENT_STATUS_BASE64_BYTES: usize = 8 * 1024;

/// Maximum accepted length of the envelope `agent` id (spec §4.1 says
/// "lowercase ASCII" identifiers such as `pi`/`codex`). Longer ids are dropped
/// so a hostile program cannot store kilobytes per tracked agent (SEC-04).
pub const MAX_AGENT_ID_BYTES: usize = 64;

/// Accepted schema version. Future versions are dropped silently until the
/// receiver is upgraded to understand them (spec §4.1 `v`).
pub const AGENT_STATUS_SCHEMA_VERSION: u32 = 1;

/// One parsed OSC 9;7 event: the envelope (`agent`, `seq`, `ts`) plus the
/// type-specific [`AgentPayload`].
///
/// The `v` version is validated during parsing and the `type` discriminator
/// is collapsed into the payload variant. Unknown future versions / types are
/// dropped during parsing (see [`parse_agent_status`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentStatusEvent {
    /// The agent identifier (envelope `agent`).
    pub agent: String,
    /// Monotonic per-(terminal, agent) sequence counter (envelope `seq`).
    pub seq: u64,
    /// Epoch milliseconds, agent clock (envelope `ts`).
    pub ts: u64,
    /// The type-specific payload (envelope `type` selects the variant).
    pub payload: AgentPayload,
}

/// Type-specific payload of an OSC 9;7 event (spec §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentPayload {
    /// `type: "state"` — agent lifecycle state.
    State(StateEvent),
    /// `type: "session"` — session identity.
    Session(SessionIdentityEvent),
    /// `type: "heartbeat"` — keepalive.
    Heartbeat(HeartbeatEvent),
    /// `type: "model"` — active model.
    Model(ModelEvent),
    /// `type: "tool_call"` — tool invocation.
    ToolCall(ToolCallEvent),
    /// `type: "file"` — file activity.
    File(FileEvent),
    /// `type: "approval"` — structured approval request.
    Approval(ApprovalEvent),
}

impl AgentPayload {
    /// The wire `type` discriminator (spec §4.2).
    pub fn type_name(&self) -> &'static str {
        match self {
            AgentPayload::State(_) => "state",
            AgentPayload::Session(_) => "session",
            AgentPayload::Heartbeat(_) => "heartbeat",
            AgentPayload::Model(_) => "model",
            AgentPayload::ToolCall(_) => "tool_call",
            AgentPayload::File(_) => "file",
            AgentPayload::Approval(_) => "approval",
        }
    }
}

impl AgentStatusEvent {
    /// The agent identifier (envelope `agent`).
    pub fn agent(&self) -> &str {
        &self.agent
    }

    /// Monotonic per-(terminal, agent) sequence counter (envelope `seq`).
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Epoch milliseconds, agent clock (envelope `ts`).
    pub fn ts(&self) -> u64 {
        self.ts
    }

    /// The wire `type` discriminator (spec §4.2).
    pub fn type_name(&self) -> &'static str {
        self.payload.type_name()
    }
}

/// Intermediate envelope used for JSON deserialization. Unknown `type`
/// values fall through to the `other` branch of the dispatch match (the
/// `type_name` is just a string), so the whole event is dropped cleanly.
#[derive(Debug, Deserialize)]
struct RawEnvelope {
    v: u32,
    agent: String,
    #[serde(rename = "type", default)]
    type_name: String,
    seq: u64,
    ts: u64,
}

/// Parse an OSC 9;7 base64 payload (the third OSC parameter, `params[2]`)
/// into a typed [`AgentStatusEvent`].
///
/// Returns `None` (silent drop, spec §3.3) on:
/// - empty/missing parameter,
/// - base64 length > [`MAX_AGENT_STATUS_BASE64_BYTES`] (spec §3.2),
/// - invalid base64,
/// - invalid UTF-8 after decode,
/// - invalid JSON,
/// - unknown schema version (`v != 1`),
/// - `agent` id empty or longer than [`MAX_AGENT_ID_BYTES`],
/// - unknown `type`,
/// - missing required envelope or per-type fields.
///
/// Unknown envelope/per-type fields are ignored (forward compatibility,
/// spec §4.1 / §4.2). `seq` dedup is **not** performed here — see the
/// module docs.
pub fn parse_agent_status(base64_param: &[u8]) -> Option<AgentStatusEvent> {
    // §3.2 size cap on the base64 length.
    if base64_param.len() > MAX_AGENT_STATUS_BASE64_BYTES {
        log::debug!(
            "OSC 9;7 dropped: base64 payload {} bytes > cap {}",
            base64_param.len(),
            MAX_AGENT_STATUS_BASE64_BYTES
        );
        return None;
    }

    // The base64 parameter arrives as raw bytes from the VT engine. It is
    // ASCII base64, so decode from the byte slice directly.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(base64_param)
        .ok()?;
    let json_str = std::str::from_utf8(&decoded).ok()?;
    parse_agent_status_json(json_str)
}

/// Parse an already-decoded JSON string into a typed [`AgentStatusEvent`].
/// Exposed for tests so the base64/UTF-8 layer can be bypassed.
pub fn parse_agent_status_json(json: &str) -> Option<AgentStatusEvent> {
    // First parse the envelope to validate `v` and read `type`. Using a
    // two-step parse (envelope, then the typed payload) keeps the per-type
    // structs clean (no `#[serde(tag = "type")]` + untagged trickery that
    // would produce worse error messages and pull in all variants).
    let raw: RawEnvelope = serde_json::from_str(json).ok()?;

    if raw.v != AGENT_STATUS_SCHEMA_VERSION {
        log::debug!("OSC 9;7 dropped: unknown schema version v={}", raw.v);
        return None;
    }

    let agent = raw.agent;
    if agent.is_empty() || agent.len() > MAX_AGENT_ID_BYTES {
        log::debug!(
            "OSC 9;7 dropped: agent id length {} outside 1..={}",
            agent.len(),
            MAX_AGENT_ID_BYTES
        );
        return None;
    }
    let seq = raw.seq;
    let ts = raw.ts;

    // Re-parse the full JSON for the typed payload. The per-type structs
    // ignore unknown fields (no `deny_unknown_fields`). Required fields
    // missing → `None` (dropped silently per §3.3).
    let value: serde_json::Value = serde_json::from_str(json).ok()?;

    macro_rules! payload {
        ($struct_name:ident, $variant:ident) => {{
            let parsed: $struct_name = serde_json::from_value(value.clone()).ok()?;
            AgentPayload::$variant(parsed)
        }};
    }

    let payload = match raw.type_name.as_str() {
        "state" => payload!(StateEvent, State),
        "session" => payload!(SessionIdentityEvent, Session),
        "heartbeat" => payload!(HeartbeatEvent, Heartbeat),
        "model" => payload!(ModelEvent, Model),
        "tool_call" => payload!(ToolCallEvent, ToolCall),
        "file" => payload!(FileEvent, File),
        "approval" => payload!(ApprovalEvent, Approval),
        other => {
            log::debug!("OSC 9;7 dropped: unknown type {other:?}");
            return None;
        }
    };
    Some(AgentStatusEvent {
        agent,
        seq,
        ts,
        payload,
    })
}

/// Build the raw OSC parameter slices for an OSC 9;7 event from a JSON string,
/// for use in downstream listener tests (base64-wraps the JSON and prepends
/// `"9"`, `"7"`). Available under `test-support` so backend crates don't need
/// a direct `base64` dependency just to exercise the OSC 9;7 path.
#[cfg(any(test, feature = "test-support"))]
pub fn encode_osc97_params(json: &str) -> Vec<Vec<u8>> {
    vec![
        b"9".to_vec(),
        b"7".to_vec(),
        base64::engine::general_purpose::STANDARD
            .encode(json.as_bytes())
            .into_bytes(),
    ]
}

#[cfg(test)]
mod receiver_tests;
