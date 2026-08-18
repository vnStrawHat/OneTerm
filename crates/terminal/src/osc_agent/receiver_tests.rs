//! Conformance tests for the OSC 9;7 receiver (spec §8).

use super::*;

use base64::Engine;

fn b64(json: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .encode(json.as_bytes())
        .into_bytes()
}

// ── Valid payloads (one per type) ──────────────────────────────
#[test]
fn parses_state_event() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"state","seq":1,"ts":1000,"state":"working","message":"thinking","session_id":"abc"}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    assert_eq!(ev.agent, "pi");
    assert_eq!(ev.seq, 1);
    assert_eq!(ev.ts, 1000);
    match &ev.payload {
        AgentPayload::State(payload) => {
            assert_eq!(payload.state, AgentState::Working);
            assert_eq!(payload.message.as_deref(), Some("thinking"));
            assert_eq!(payload.session_id.as_deref(), Some("abc"));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(ev.type_name(), "state");
    assert_eq!(ev.agent(), "pi");
    assert_eq!(ev.seq(), 1);
    assert_eq!(ev.ts(), 1000);
}

#[test]
fn parses_session_event() {
    let p = b64(
        r#"{"v":1,"agent":"codex","type":"session","seq":2,"ts":2000,"session_id":"s1","reason":"startup","parent_id":"p0","project_dir":"/opt/app/dev/myProject"}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    assert_eq!(ev.agent, "codex");
    match ev.payload {
        AgentPayload::Session(payload) => {
            assert_eq!(payload.session_id, "s1");
            assert_eq!(payload.reason.as_deref(), Some("startup"));
            assert_eq!(payload.parent_id.as_deref(), Some("p0"));
            assert_eq!(
                payload.project_dir.as_deref(),
                Some("/opt/app/dev/myProject")
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_heartbeat_event() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"heartbeat","seq":3,"ts":3000,"interval_ms":15000,"state":"idle"}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    match ev.payload {
        AgentPayload::Heartbeat(payload) => {
            assert_eq!(payload.interval_ms, Some(15000));
            assert_eq!(payload.state, Some(AgentState::Idle));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_model_event() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"model","seq":4,"ts":4000,"provider":"anthropic","model_id":"claude-sonnet-4","model_name":"Claude Sonnet 4","context_window":200000,"max_output_tokens":8192,"reasoning":true,"source":"set","context_used":84500}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    match ev.payload {
        AgentPayload::Model(payload) => {
            assert_eq!(payload.provider, "anthropic");
            assert_eq!(payload.model_id, "claude-sonnet-4");
            assert_eq!(payload.model_name.as_deref(), Some("Claude Sonnet 4"));
            assert_eq!(payload.context_window, Some(200000));
            assert_eq!(payload.max_output_tokens, Some(8192));
            assert_eq!(payload.reasoning, Some(true));
            assert_eq!(payload.source, Some(ModelSource::Set));
            assert_eq!(payload.context_used, Some(84500));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_tool_call_start_and_end() {
    let start = b64(
        r#"{"v":1,"agent":"pi","type":"tool_call","seq":10,"ts":5000,"tool_call_id":"tc-42","tool":"bash","phase":"start","target":"src/app.rs","args":"grep -n TODO src/app.rs","args_redacted":false}"#,
    );
    let ev = parse_agent_status(&start).unwrap();
    match ev.payload {
        AgentPayload::ToolCall(payload) => {
            assert_eq!(payload.tool_call_id, "tc-42");
            assert_eq!(payload.tool, "bash");
            assert_eq!(payload.phase, ToolCallPhase::Start);
            assert_eq!(payload.target.as_deref(), Some("src/app.rs"));
            assert_eq!(payload.args_redacted, Some(false));
        }
        other => panic!("unexpected {other:?}"),
    }

    let end = b64(
        r#"{"v":1,"agent":"pi","type":"tool_call","seq":11,"ts":5900,"tool_call_id":"tc-42","tool":"bash","phase":"end","exit_code":0,"is_error":false,"duration_ms":900}"#,
    );
    let ev = parse_agent_status(&end).unwrap();
    match ev.payload {
        AgentPayload::ToolCall(payload) => {
            assert_eq!(payload.phase, ToolCallPhase::End);
            assert_eq!(payload.exit_code, Some(0));
            assert_eq!(payload.is_error, Some(false));
            assert_eq!(payload.duration_ms, Some(900));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_file_event() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"file","seq":12,"ts":6000,"path":"src/app.rs","action":"edit","tool_call_id":"tc-43"}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    match ev.payload {
        AgentPayload::File(payload) => {
            assert_eq!(payload.path, "src/app.rs");
            assert_eq!(payload.action, FileAction::Edit);
            assert_eq!(payload.tool_call_id.as_deref(), Some("tc-43"));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn parses_approval_event() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"approval","seq":13,"ts":7000,"id":"apr-7","kind":"permission","prompt":"Allow bash?","options":["yes","no","always"],"default":"no","tool":"bash","tool_call_id":"tc-44","risk":"high","timeout_ms":0}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    match ev.payload {
        AgentPayload::Approval(payload) => {
            assert_eq!(payload.id, "apr-7");
            assert_eq!(payload.kind, ApprovalKind::Permission);
            assert_eq!(payload.prompt, "Allow bash?");
            assert_eq!(
                payload.options.as_deref(),
                Some(&["yes".to_string(), "no".into(), "always".into()][..])
            );
            assert_eq!(payload.default.as_deref(), Some("no"));
            assert_eq!(payload.tool.as_deref(), Some("bash"));
            assert_eq!(payload.risk, Some(ApprovalRisk::High));
            assert_eq!(payload.timeout_ms, Some(0));
        }
        other => panic!("unexpected {other:?}"),
    }
}

// ── §7 regression: JSON value containing ';' ────────────────────
#[test]
fn semicolon_in_string_value_survives_base64_wrap() {
    // A `;` inside a JSON string would break a raw-JSON OSC (it would be
    // split as a parameter separator). Base64-wrapping keeps it intact.
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"state","seq":1,"ts":1,"state":"error","message":"retry: https://x/y?a=1;b=2"}"#,
    );
    let ev = parse_agent_status(&p).unwrap();
    match ev.payload {
        AgentPayload::State(payload) => {
            assert_eq!(payload.state, AgentState::Error);
            assert_eq!(
                payload.message.as_deref(),
                Some("retry: https://x/y?a=1;b=2")
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

// ── Malformed payloads → dropped silently ──────────────────────
#[test]
fn invalid_base64_dropped() {
    assert!(parse_agent_status(b"!!!not base64!!!").is_none());
}

#[test]
fn invalid_utf8_dropped() {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64(
            r#"{"v":1,"agent":"pi","type":"state","seq":1,"ts":1,"state":"idle"}"#,
        ))
        .unwrap();
    let mut bad = raw.clone();
    bad[0] = 0xFF; // corrupt first byte → invalid UTF-8
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bad);
    assert!(parse_agent_status(encoded.as_bytes()).is_none());
}

#[test]
fn invalid_json_dropped() {
    // Valid base64 but not valid JSON.
    let p = base64::engine::general_purpose::STANDARD
        .encode(b"{not json")
        .into_bytes();
    assert!(parse_agent_status(&p).is_none());
}

#[test]
fn unknown_schema_version_dropped() {
    let p = b64(r#"{"v":2,"agent":"pi","type":"state","seq":1,"ts":1,"state":"idle"}"#);
    assert!(parse_agent_status(&p).is_none());
}

#[test]
fn unknown_type_dropped() {
    let p = b64(r#"{"v":1,"agent":"pi","type":"future_thing","seq":1,"ts":1}"#);
    assert!(parse_agent_status(&p).is_none());
}

#[test]
fn extra_envelope_fields_ignored() {
    let p = b64(
        r#"{"v":1,"agent":"pi","type":"state","seq":1,"ts":1,"state":"idle","future_field":"xyz"}"#,
    );
    assert!(parse_agent_status(&p).is_some());
}

#[test]
fn missing_required_state_field_dropped() {
    // `state` is required for type "state".
    let p = b64(r#"{"v":1,"agent":"pi","type":"state","seq":1,"ts":1}"#);
    assert!(parse_agent_status(&p).is_none());
}

#[test]
fn missing_required_envelope_fields_dropped() {
    // No `seq`.
    let p = b64(r#"{"v":1,"agent":"pi","type":"state","ts":1,"state":"idle"}"#);
    assert!(parse_agent_status(&p).is_none());
    // No `agent`.
    let p = b64(r#"{"v":1,"type":"state","seq":1,"ts":1,"state":"idle"}"#);
    assert!(parse_agent_status(&p).is_none());
}

// ── §3.2 size cap ──────────────────────────────────────────────
#[test]
fn payload_at_8kib_accepted() {
    // Build a state event whose base64 encoding is exactly 8 KiB.
    // Pad the `message` field with filler to hit the cap.
    let mut msg = "x".repeat(6000);
    loop {
        let json = format!(
            r#"{{"v":1,"agent":"pi","type":"state","seq":1,"ts":1,"state":"idle","message":"{}"}}"#,
            msg
        );
        let enc = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
        match enc.len().cmp(&MAX_AGENT_STATUS_BASE64_BYTES) {
            std::cmp::Ordering::Less => msg.push('x'),
            std::cmp::Ordering::Equal => {
                assert!(parse_agent_status(enc.as_bytes()).is_some());
                return;
            }
            std::cmp::Ordering::Greater => panic!("overshot cap"),
        }
    }
}

#[test]
fn payload_over_8kib_dropped() {
    let json = format!(
        r#"{{"v":1,"agent":"pi","type":"state","seq":1,"ts":1,"state":"idle","message":"{}"}}"#,
        "x".repeat(9000)
    );
    let enc = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
    assert!(enc.len() > MAX_AGENT_STATUS_BASE64_BYTES);
    assert!(parse_agent_status(enc.as_bytes()).is_none());
}

#[test]
fn empty_param_dropped() {
    assert!(parse_agent_status(b"").is_none());
}

// ── Forward compatibility ───────────────────────────────────────
#[test]
fn unknown_type_with_extra_fields_dropped() {
    let p = b64(r#"{"v":1,"agent":"pi","type":"mystery","seq":1,"ts":1,"anything":42}"#);
    assert!(parse_agent_status(&p).is_none());
}

#[test]
fn agent_state_badge() {
    assert_eq!(AgentState::Working.badge(), "🟢");
    assert_eq!(AgentState::Blocked.badge(), "🟠");
    assert_eq!(AgentState::Idle.badge(), "⚪");
    assert_eq!(AgentState::Done.badge(), "✅");
    assert_eq!(AgentState::Error.badge(), "❌");
}

// ── seq dedup (spec §4.1 / §8.3) ───────────────────────────────
/// Build a `state` event with the given agent + seq (ts fixed at 1).
fn state_event(agent: &str, seq: u64) -> AgentStatusEvent {
    parse_agent_status_json(&format!(
        "{{\"v\":1,\"agent\":\"{agent}\",\"type\":\"state\",\"seq\":{seq},\"ts\":1,\"state\":\"idle\"}}",
    ))
    .expect("valid state event")
}

#[test]
fn dedup_applies_newer_seq_and_updates_watermark() {
    let mut last = AgentSeqWatermarks::default();
    assert!(should_apply(&mut last, &state_event("pi", 5)));
    assert_eq!(last.get("pi"), Some(5));
    // newer seq applied.
    assert!(should_apply(&mut last, &state_event("pi", 6)));
    assert_eq!(last.get("pi"), Some(6));
}

#[test]
fn dedup_drops_equal_seq() {
    let mut last = AgentSeqWatermarks::default();
    assert!(should_apply(&mut last, &state_event("pi", 5)));
    // equal seq dropped, watermark unchanged.
    assert!(!should_apply(&mut last, &state_event("pi", 5)));
    assert_eq!(last.get("pi"), Some(5));
}

#[test]
fn dedup_drops_stale_seq() {
    let mut last = AgentSeqWatermarks::default();
    assert!(should_apply(&mut last, &state_event("pi", 5)));
    // older seq dropped.
    assert!(!should_apply(&mut last, &state_event("pi", 4)));
    assert_eq!(last.get("pi"), Some(5));
}

#[test]
fn dedup_is_per_agent() {
    let mut last = AgentSeqWatermarks::default();
    assert!(should_apply(&mut last, &state_event("pi", 1)));
    // different agent, same seq — must NOT be deduped against pi.
    assert!(should_apply(&mut last, &state_event("codex", 1)));
    assert_eq!(last.get("pi"), Some(1));
    assert_eq!(last.get("codex"), Some(1));
    // pi seq=1 again — deduped against pi's last (1).
    assert!(!should_apply(&mut last, &state_event("pi", 1)));
}

#[test]
fn dedup_first_event_applied_from_empty_state() {
    let mut last = AgentSeqWatermarks::default();
    // seq=0 from a fresh agent — still > default watermark (0)? No: 0 <= 0.
    assert!(!should_apply(&mut last, &state_event("pi", 0)));
    // seq=1 from a fresh agent — applied.
    assert!(should_apply(&mut last, &state_event("pi", 1)));
}

/// SEC-04: the watermark table never grows past `MAX_TRACKED_AGENTS`; the
/// least recently seen id is evicted, and an evicted id starts over at
/// watermark 0 (a replay of its old seq is accepted again — bounded memory is
/// worth more than perfect dedup for ids we have not seen in a long time).
#[test]
fn dedup_table_is_bounded_and_evicts_least_recent() {
    let mut last = AgentSeqWatermarks::default();
    for i in 0..MAX_TRACKED_AGENTS {
        assert!(should_apply(&mut last, &state_event(&format!("a{i}"), 1)));
    }
    assert_eq!(last.len(), MAX_TRACKED_AGENTS);
    // Touch a0 so a1 becomes the least recently seen.
    assert!(!should_apply(&mut last, &state_event("a0", 1)));
    // A new id evicts a1, not a0.
    assert!(should_apply(&mut last, &state_event("newcomer", 1)));
    assert_eq!(last.len(), MAX_TRACKED_AGENTS);
    assert_eq!(last.get("a0"), Some(1));
    assert_eq!(last.get("a1"), None);
    assert_eq!(last.get("newcomer"), Some(1));
    // Hostile churn: many distinct ids never grow the table.
    for i in 0..10_000 {
        should_apply(&mut last, &state_event(&format!("x{i}"), 1));
    }
    assert_eq!(last.len(), MAX_TRACKED_AGENTS);
}

/// SEC-04: over-long agent ids are dropped at parse time.
#[test]
fn oversized_agent_id_is_dropped_at_parse() {
    let ok = "a".repeat(MAX_AGENT_ID_BYTES);
    assert!(parse_agent_status_json(&format!(
        "{{\"v\":1,\"agent\":\"{ok}\",\"type\":\"state\",\"seq\":1,\"ts\":1,\"state\":\"idle\"}}"
    )).is_some());
    let too_long = "a".repeat(MAX_AGENT_ID_BYTES + 1);
    assert!(parse_agent_status_json(&format!(
        "{{\"v\":1,\"agent\":\"{too_long}\",\"type\":\"state\",\"seq\":1,\"ts\":1,\"state\":\"idle\"}}"
    )).is_none());
    assert!(
        parse_agent_status_json(
            "{\"v\":1,\"agent\":\"\",\"type\":\"state\",\"seq\":1,\"ts\":1,\"state\":\"idle\"}"
        )
        .is_none()
    );
}
