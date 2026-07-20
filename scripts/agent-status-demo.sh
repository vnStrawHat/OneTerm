#!/usr/bin/env bash
# Simulate coding-agent activity in OneTerm by emitting OSC 9;7 events.
#
# Run this script inside a OneTerm terminal. The default scenario exercises the
# folded Agent Panel card: session, model/context, lifecycle, tool, file, and
# approval data. Run the `multi` scenario in one Space to verify that multiple
# agent IDs produce separate cards. Run the script in terminals from different
# Tabs/Spaces to verify grouping.
#
# Usage:
#   scripts/agent-status-demo.sh [scenario] [agent] [delay_ms]
#
# Scenarios: demo, working, blocked, idle, error, done, multi
# Examples:
#   scripts/agent-status-demo.sh
#   scripts/agent-status-demo.sh blocked codex 1000
#   scripts/agent-status-demo.sh multi
#   scripts/agent-status-demo.sh demo pi 0 > /tmp/agent-status-osc.bin

set -euo pipefail

readonly ESC=$'\033'
readonly BEL=$'\007'

SCENARIO="${1:-demo}"
AGENT="${2:-pi}"
DELAY_MS="${3:-600}"
SESSION_ID=""
SEQ=0

usage() {
    cat <<'EOF'
Usage: scripts/agent-status-demo.sh [scenario] [agent] [delay_ms]

Scenarios:
  demo      Exercise all Agent Panel sections, ending in idle (default)
  working   Leave one card working with a running tool
  blocked   Leave one card blocked with a pending approval
  idle      Leave one card idle
  error     Leave one card in an error state
  done      Leave one card in a done state
  multi     Create working, blocked, and idle cards in the same Space

The agent identifier must start with a lowercase ASCII letter and contain only
lowercase letters, digits, underscores, or hyphens. Set delay_ms to 0 for an
instant run.
EOF
}

fail() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

validate_agent() {
    [[ "$1" =~ ^[a-z][a-z0-9_-]*$ ]] || fail "invalid agent identifier: $1"
}

now_ms() {
    local value
    value="$(date +%s%3N 2>/dev/null || true)"
    if [[ "$value" =~ ^[0-9]{13}$ ]]; then
        printf '%s' "$value"
    else
        printf '%s000' "$(date +%s)"
    fi
}

sleep_step() {
    (( DELAY_MS == 0 )) && return
    local seconds
    printf -v seconds '%d.%03d' "$((DELAY_MS / 1000))" "$((DELAY_MS % 1000))"
    sleep "$seconds"
}

emit_event() {
    local event_type="$1"
    local fields="$2"
    local description="$3"
    local timestamp json payload

    timestamp="$(now_ms)"
    if (( SEQ == 0 )); then
        # Date.now() * 1000 + counter, as recommended by the protocol.
        SEQ=$((timestamp * 1000))
    fi
    SEQ=$((SEQ + 1))

    json="{\"v\":1,\"agent\":\"${AGENT}\",\"type\":\"${event_type}\",\"seq\":${SEQ},\"ts\":${timestamp}${fields}}"
    payload="$(printf '%s' "$json" | base64 | tr -d '\r\n')"
    (( ${#payload} <= 8192 )) || fail "encoded payload exceeds the OSC 9;7 size cap"

    printf '%s]9;7;%s%s' "$ESC" "$payload" "$BEL"
    printf '  %-10s %s [%s]\n' "$event_type" "$description" "$AGENT"
    sleep_step
}

begin_agent() {
    AGENT="$1"
    validate_agent "$AGENT"
    SESSION_ID="${AGENT}-demo-$$"

    emit_event session \
        ",\"session_id\":\"${SESSION_ID}\",\"reason\":\"startup\"" \
        "session ${SESSION_ID}"
}

emit_model() {
    local provider="$1"
    local model_id="$2"
    local model_name="$3"
    local context_used="$4"
    local reasoning="$5"

    emit_event model \
        ",\"provider\":\"${provider}\",\"model_id\":\"${model_id}\",\"model_name\":\"${model_name}\",\"context_window\":200000,\"max_output_tokens\":8192,\"reasoning\":${reasoning},\"source\":\"set\",\"context_used\":${context_used}" \
        "${model_name}, ${context_used}/200000 tokens"
}

emit_state() {
    local state="$1"
    local message="$2"

    emit_event state \
        ",\"state\":\"${state}\",\"message\":\"${message}\",\"session_id\":\"${SESSION_ID}\"" \
        "${state}: ${message}"
}

scenario_demo() {
    begin_agent "$AGENT"
    emit_model "anthropic" "claude-sonnet-4" "Claude Sonnet 4" 84500 true
    emit_state working "Analyzing the workspace"
    emit_event heartbeat ',"interval_ms":5000,"state":"working"' "working keepalive"

    emit_event tool_call \
        ',"tool_call_id":"tc-demo-1","tool":"bash","phase":"start","target":"src/app.rs","args":"grep -n TODO src/app.rs; cargo check","args_redacted":false' \
        "bash started"
    emit_event tool_call \
        ',"tool_call_id":"tc-demo-1","tool":"bash","phase":"update","progress":"Checking dependencies (42%)"' \
        "bash progress 42%"
    emit_event tool_call \
        ',"tool_call_id":"tc-demo-1","tool":"bash","phase":"end","exit_code":0,"is_error":false,"duration_ms":900,"diff_stat":"+12 -3"' \
        "bash completed"
    emit_event file \
        ',"path":"src/app.rs","action":"edit","tool_call_id":"tc-demo-1"' \
        "edited src/app.rs"

    emit_state blocked "Waiting for permission"
    emit_event approval \
        ',"id":"apr-demo-1","kind":"permission","prompt":"Allow bash to run `cargo test --workspace`?","options":["yes","no","always"],"default":"no","tool":"bash","tool_call_id":"tc-demo-2","risk":"medium","timeout_ms":0' \
        "approval requested"

    emit_state working "Permission received; continuing"
    emit_event model \
        ',"provider":"anthropic","model_id":"claude-sonnet-4","model_name":"Claude Sonnet 4","context_window":200000,"max_output_tokens":8192,"reasoning":true,"source":"set","context_used":121000' \
        "context updated to 121000/200000"
    emit_state error "Simulated non-retryable provider error"
    emit_state idle "Ready for the next prompt"
}

scenario_working() {
    begin_agent "$AGENT"
    emit_model "anthropic" "claude-sonnet-4" "Claude Sonnet 4" 84500 true
    emit_state working "Running workspace checks"
    emit_event tool_call \
        ',"tool_call_id":"tc-working-1","tool":"bash","phase":"start","target":"cargo test --workspace","args":"cargo test --workspace","args_redacted":false' \
        "long-running workspace test"
    emit_event heartbeat ',"interval_ms":5000,"state":"working"' "working keepalive"
}

scenario_blocked() {
    begin_agent "$AGENT"
    emit_model "openai" "gpt-5-codex" "GPT-5 Codex" 64000 true
    emit_state blocked "Waiting for permission"
    emit_event approval \
        ',"id":"apr-blocked-1","kind":"permission","prompt":"Allow the agent to modify Cargo.toml?","options":["yes","no","always"],"default":"no","tool":"edit","tool_call_id":"tc-blocked-1","risk":"high","timeout_ms":0' \
        "high-risk approval requested"
}

scenario_idle() {
    begin_agent "$AGENT"
    emit_model "anthropic" "claude-sonnet-4" "Claude Sonnet 4" 42000 true
    emit_state idle "Ready for the next prompt"
}

scenario_error() {
    begin_agent "$AGENT"
    emit_model "openai" "gpt-5-codex" "GPT-5 Codex" 91000 true
    emit_state error "Simulated authentication failure"
}

scenario_done() {
    begin_agent "$AGENT"
    emit_model "local" "demo-model" "Demo Model" 12000 false
    emit_state done "Session completed successfully"
}

scenario_multi() {
    begin_agent pi
    emit_model "anthropic" "claude-sonnet-4" "Claude Sonnet 4" 84500 true
    emit_state working "Reviewing source code"
    emit_event tool_call \
        ',"tool_call_id":"tc-multi-pi","tool":"read","phase":"start","target":"crates/terminal/src/osc.rs","args":"Read OSC parser","args_redacted":false' \
        "reading OSC parser"

    begin_agent codex
    emit_model "openai" "gpt-5-codex" "GPT-5 Codex" 132000 true
    emit_state blocked "Waiting for permission"
    emit_event approval \
        ',"id":"apr-multi-codex","kind":"confirm","prompt":"Run the full workspace quality gate?","options":["yes","no"],"default":"yes","tool":"bash","risk":"medium","timeout_ms":0' \
        "confirmation requested"

    begin_agent claude
    emit_model "anthropic" "claude-opus-4" "Claude Opus 4" 38000 true
    emit_state idle "Ready for the next prompt"
}

if [[ "$SCENARIO" == "-h" || "$SCENARIO" == "--help" || "$SCENARIO" == "help" ]]; then
    usage
    exit 0
fi

[[ "$DELAY_MS" =~ ^[0-9]+$ ]] || fail "delay_ms must be a non-negative integer"
validate_agent "$AGENT"

printf 'OneTerm OSC 9;7 agent-status simulation: %s\n' "$SCENARIO"
case "$SCENARIO" in
    demo)    scenario_demo ;;
    working) scenario_working ;;
    blocked) scenario_blocked ;;
    idle)    scenario_idle ;;
    error)   scenario_error ;;
    done)    scenario_done ;;
    multi)   scenario_multi ;;
    *) usage >&2; fail "unknown scenario: $SCENARIO" ;;
esac
printf 'Simulation complete. Open the Agent Panel to inspect the resulting card(s).\n'
