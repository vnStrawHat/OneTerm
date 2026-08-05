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
            space_index: 0,
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
