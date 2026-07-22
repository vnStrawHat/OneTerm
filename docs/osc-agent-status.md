# OSC 9;7 — Agent Status Event

> Protocol specification for a coding-agent status channel carried over an
> OSC (Operating System Command) escape sequence. Any terminal that parses
> OSC 9 and any agent that can write bytes to its own stdout can implement it.

---

## 1. Purpose

A coding agent (Claude Code, Codex, pi, Cursor, …) runs inside a terminal
emulator. The terminal already sees every byte the agent prints. This spec
defines a small, structured side-channel — an OSC — that an agent emits to
report its own lifecycle state, session identity, and (optionally) tool /
file activity to whatever is hosting the terminal.

The channel is **one-directional** (agent → terminal host). It is sufficient
for monitoring/watchdog UIs (a fleet view of "working / blocked / idle"). It
deliberately does not define a reply path: if a host wants to act on an agent
(approve a prompt, interrupt it), it does so through the terminal's own input
channel, not through OSC.

### 1.1 Why an OSC and not a sidecar socket

1. **Zero infrastructure.** No socket server, no named pipe, no RPC schema, no
   per-instance path discovery. Any terminal that already parses OSC 9 gains
   the monitor backend by adding one parser branch.
2. **Works across remote shells with no extra work.** An agent running on a
   remote host emits OSC 9;7 into its stdout; the bytes flow back through the
   SSH channel and hit the host terminal's parser unchanged. A socket would
   need an SSH tunnel or a TCP relay.
3. **Natural per-terminal multiplexing.** An OSC is emitted by the process
   inside a specific terminal, so the host maps it to that terminal
   automatically — no `pane_id` / `terminal_id` field is required in the
   payload.
4. **Zero-config opt-in.** An agent just writes bytes to stdout. No env-var
   handshake. Terminals that do not implement OSC 9;7 drop it silently (it is
   a private extension), so the same agent works inside any terminal without
   harm.

---

## 2. Numbering choice

The OSC number space is shared. The choice here is **OSC 9;7** (a sub-code of
the OSC 9 family).

| Candidate | Verdict |
|---|---|
| `OSC 9;7` (sub-code of OSC 9 family) | **Chosen.** OSC 9 is widely implemented for desktop notifications (`9;msg`) and progress (`9;4;st;pr`). Sub-codes `0..4` are taken (ConEmu misc + progress); `7` is free and reads naturally as "agent event". Unknown terminals ignore it. |
| `OSC 1337;Agent` (iTerm2 vendor) | Rejected — 1337 carries unrelated iTerm2 semantics (inline images, etc.). |
| `OSC 633;Agent` (VS Code) | Rejected — 633 is VS Code shell-integration-specific. |
| `OSC 777;…` (urxvt 3-field) | Rejected — would require a new parser branch for the 777 family. |
| Brand-new number (e.g. `OSC 9001`) | Rejected — breaks the "lives in the OSC 9 family" reading and gains nothing, since compatibility with other terminals is zero either way for a private extension. |

**Decision: `OSC 9;7`.** It is a private extension; no other terminal is
expected to interpret it. Other terminals that already parse OSC 9 will treat
`9;7` as an unknown notification sub-code and ignore it.

---

## 3. Wire format

```
ESC ] 9 ; 7 ; <base64-json> ST
```

- `ESC ]` = `\x1b]` — OSC opener.
- `9` — OSC number (the notification/progress family).
- `7` — sub-code selecting the agent-status event.
- `<base64-json>` — the standard-base64 encoding of a UTF-8 JSON object
  (see §4). Base64 contains no `;`, so it survives the VT engine's
  `;`-splitting intact (see §3.1). Must not contain the ST terminator bytes.
- `ST` — String Terminator. **Use `BEL` (`\x07`)** for maximum compatibility.
  `ESC \` (`\x1b\\`) is also accepted.

### 3.1 The payload is always base64-wrapped

Most VT engines split the OSC into parameters on `;` before the application
sees them. (In xterm-derived parsers, byte `0x3B` (`;`) finalises the current
parameter slice and starts a new one; every other byte appends to the current
parameter's raw buffer.) Therefore the JSON payload **cannot be sent raw** —
any `;` inside a JSON value would be broken into a separate parameter field
and could not be recovered by the receiver.

Wire form is therefore **always** base64-wrapped:

```
ESC ] 9 ; 7 ; <base64(json)> ST
```

- The agent encodes `JSON.stringify(payload)` with standard base64
  (`base64::engine::general_purpose::STANDARD` / `Buffer.from(...).toString("base64")`
  / equivalent) and writes `\x1b]9;7;<b64>\x07`.
- The receiver detects the `7` sub-code, takes the third parameter,
  base64-decodes it, then JSON-parses the result. There is only one wire form.
- Trade-off: ~33 % size overhead. Acceptable for status events (typical
  payload < 300 bytes; even the largest `model` event is well under
  1 KiB raw).

> Why not re-join the split parameters with `;`? That only works for
> free-form text (e.g. a notification message), where re-joining is
> semantically acceptable. JSON re-joining would silently corrupt any value
> that contained `;` (a timestamp string, an error message, a URL with query
> params). Base64 sidesteps it entirely.

### 3.2 Size cap

The receiver **MUST** cap OSC 9;7 payloads at **8 KiB** (measured on the
base64 length, i.e. ~6 KiB of raw JSON after decode). Oversized payloads are
dropped silently (with a debug log). This prevents a malicious or buggy agent
from flooding the terminal with multi-megabyte OSCs. Agent status payloads are
small (< 1 KiB typical, < 4 KiB worst case), so the cap never hits legitimate
use.

### 3.3 Malformed-payload handling

On any of: invalid base64, invalid UTF-8, invalid JSON, unknown schema
version, unknown `type` — the receiver **MUST** drop the event silently (a
debug log is allowed). The terminal must never crash or render artefacts on
a malformed agent OSC.

---

## 4. Payload schema

### 4.1 Envelope

Every OSC 9;7 payload is a JSON object with a common envelope:

```json
{
  "v": 1,
  "agent": "pi",
  "type": "state",
  "seq": 12345,
  "ts": 1721234567890
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `v` | integer | yes | Schema version. Starts at `1`. Receivers accept `1` and silently drop unknown future versions until upgraded to understand them. |
| `agent` | string | yes | Agent identifier: `"pi"`, `"codex"`, `"claude"`, `"cursor"`, `"factory"`, `"aider"`, … Lowercase ASCII. Used for icon/label selection by the host. |
| `type` | string | yes | Event type discriminator (see §4.2). |
| `seq` | integer | yes | Monotonic per-(terminal, agent) sequence counter. The receiver ignores any event whose `seq` is `<=` the last applied `seq` for that (terminal, agent) pair. This discards out-of-order or duplicate reports (e.g. a late retry after a reload). The agent **must** make `seq` strictly increasing within a session; a reasonable choice is `Date.now() * 1000 + counter`. |
| `ts` | integer | yes | Epoch milliseconds (agent clock). Used for stale detection (§5.3). |

Unknown envelope fields **must be ignored** (forward compatibility). The
receiver must not fail on extra keys.

### 4.2 Event types

The protocol defines the following event types. They are grouped by
concern only for readability; on the wire they are all peers discriminated
by `type`. An agent MAY emit any subset; a receiver MUST ignore types it
does not recognise (forward compatibility).

| `type` | Group | Purpose |
|---|---|---|
| `state` | lifecycle | Agent lifecycle state (working/blocked/idle/done/error). Core event. |
| `session` | lifecycle | Session identity for resume after restart. |
| `heartbeat` | lifecycle | Keepalive so the host can distinguish "idle" from "frozen". |
| `model` | context | Active model/provider/context window. |
| `tool_call` | action | Tool invocation start/update/end with args/error. |
| `file` | action | File activity (read/edit/write/delete). |
| `approval` | action | Structured approval/permission request (the reason for `state: blocked`). |

Every event carries the §4.1 envelope (`v`, `agent`, `type`, `seq`, `ts`).
The tables below list only the type-specific fields.

---

#### 4.2.1 `type: "state"` — agent lifecycle state

The core event. The agent is the authority for its own state when it emits
OSC 9;7 (see §5).

```json
{
  "v": 1, "agent": "pi", "type": "state",
  "seq": 12346, "ts": 1721234569000,
  "state": "working",
  "message": "thinking",
  "session_id": "abc"
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `state` | string | yes | One of `"working"`, `"blocked"`, `"idle"`, `"done"`, `"error"`. See §5.1. |
| `message` | string | no | Short human-readable detail (e.g. the retryable error string for `error`, or the approval prompt for `blocked`). Cap 256 chars; longer is truncated. |
| `session_id` | string | no | Agent session id, for resume. Same value reported by a `session` event (§4.2.2); repeating it on every state event is allowed but not required. |

---

#### 4.2.2 `type: "session"` — session identity

Emitted at agent start and again after a reload/fork/resume so the host can
offer "resume this agent session" even if the terminal process has died and
been restarted.

```json
{
  "v": 1, "agent": "pi", "type": "session",
  "seq": 12300, "ts": 1721234567000,
  "session_id": "abc",
  "reason": "startup",
  "parent_id": "parent-session-id",
  "project_dir": "/work/my-project"
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `session_id` | string | yes | Opaque session id (the canonical handle for resume and for cross-references such as `state.session_id` and `parent_id`). |
| `reason` | string | no | `"startup"` \| `"reload"` \| `"new"` \| `"resume"` \| `"fork"`. Display-only. |
| `parent_id` | string | no | The `session_id` of the parent session, for fork/clone lineage. Absent for a root session. |
| `project_dir` | string | no | Absolute or agent-native path of the project directory / cwd the agent is currently working in. Emit on session start/reload/resume/fork so the host can label or group agents by project. |

---

#### 4.2.3 `type: "heartbeat"` — keepalive

Emitted periodically while the agent is alive, even when idle, so the host
can distinguish "idle and healthy" from "frozen / crashed without an exit"
without waiting for the full stale threshold.

```json
{
  "v": 1, "agent": "pi", "type": "heartbeat",
  "seq": 12700, "ts": 1721234580000,
  "interval_ms": 15000,
  "state": "idle"
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `interval_ms` | integer | no | The cadence the agent intends to emit heartbeats at. The host may use this to set its stale timer more tightly than the global `stale_threshold_ms`. |
| `state` | string | no | Current lifecycle state (same values as `state.state`). Optional convenience so a heartbeat can also serve as a low-priority state refresh. |

Recommended cadence: every 15 s while idle, every 5 s while working. The
agent SHOULD emit a heartbeat shortly after each `state` event so the host
never waits a full interval to learn the agent is alive.

---

#### 4.2.4 `type: "model"` — active model

Emitted when the active model changes (selection, cycling, session restore),
once at startup, and whenever context usage changes meaningfully (see the
emit condition below). Carries the context window and the current context
usage so the host can render a context-usage bar without a separate metering
event.

```json
{
  "v": 1, "agent": "pi", "type": "model",
  "seq": 12800, "ts": 1721234581000,
  "provider": "anthropic",
  "model_id": "claude-sonnet-4",
  "model_name": "Claude Sonnet 4",
  "context_window": 200000,
  "max_output_tokens": 8192,
  "reasoning": true,
  "source": "set",
  "context_used": 84500
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `provider` | string | yes | Provider id, e.g. `"anthropic"`, `"openai"`, `"local"`. |
| `model_id` | string | yes | Provider-specific model id. |
| `model_name` | string | no | Human-readable display name. Defaults to `model_id`. |
| `context_window` | integer | no | Model context window in tokens. |
| `max_output_tokens` | integer | no | Max output tokens per response. |
| `reasoning` | boolean | no | Whether the model supports/has enabled extended reasoning. |
| `source` | string | no | `"set"` \| `"cycle"` \| `"restore"`. Display-only. |
| `context_used` | integer | no | Tokens currently in context. Lets the host render a usage bar relative to `context_window`. The agent SHOULD update it on the same emit cadence as the metering (see emit condition below). |

**Emit condition.** The agent SHOULD emit a `model` event:
1. once at startup,
2. whenever the active model changes (selection, cycling, session restore), and
3. after each model response, when `context_used` has changed enough that the
   host's usage bar would move (e.g. a relative change of ≥10%, or at least once
   per turn). This cadence replaces a dedicated metering event: a receiver that
   tracks `context_used` over time can render usage progress without a separate
   `usage` event. A receiver MUST accept a `model` event at any time and MUST
   treat the latest `context_used` as the current context usage.

---

#### 4.2.5 `type: "tool_call"` — tool invocation

Emitted at tool start, optional progress updates, and tool end. The
`tool_call_id` correlates start/update/end for the same invocation.

```json
{
  "v": 1, "agent": "pi", "type": "tool_call",
  "seq": 13100, "ts": 1721234584000,
  "tool_call_id": "tc-42",
  "tool": "bash",
  "phase": "start",
  "target": "src/app.rs",
  "args": "grep -n TODO src/app.rs",
  "args_redacted": false
}
```

```json
{
  "v": 1, "agent": "pi", "type": "tool_call",
  "seq": 13150, "ts": 1721234584900,
  "tool_call_id": "tc-42",
  "tool": "bash",
  "phase": "end",
  "exit_code": 0,
  "is_error": false,
  "duration_ms": 900
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `tool_call_id` | string | yes | Correlation id for start/update/end of one invocation. |
| `tool` | string | yes | Tool name: `"bash"`, `"edit"`, `"write"`, `"read"`, `"web_search"`, … |
| `phase` | string | yes | `"start"` \| `"update"` \| `"end"`. |
| `target` | string | no | Primary target (file path, command, URL, query). For `"start"`. |
| `args` | string | no | Short argument summary. The agent SHOULD redact secrets (see §4.3). |
| `args_redacted` | boolean | no | `true` if `args` was redacted/truncated. |
| `exit_code` | integer | no | For `"end"` of `bash`-like tools. |
| `is_error` | boolean | no | For `"end"`. |
| `error_message` | string | no | For `"end"` when `is_error`. |
| `duration_ms` | integer | no | For `"end"`: wall-clock duration. |
| `diff_stat` | string | no | `"+N -M"` for edit/write tools. |
| `progress` | string | no | For `"update"`: a short progress line (e.g. `"42%"`, `"downloading…"`). |

---

#### 4.2.6 `type: "file"` — file activity

A compact file-event feed, independent of `tool_call` (a single `edit` tool
call produces one `tool_call` end + one `file` event). Lets the host render a
"recently modified by agents" list without parsing tool args.

```json
{
  "v": 1, "agent": "pi", "type": "file",
  "seq": 13200, "ts": 1721234585000,
  "path": "src/app.rs",
  "action": "edit",
  "tool_call_id": "tc-43"
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `path` | string | yes | File path (relative to cwd or absolute). |
| `action` | string | yes | `"read"` \| `"edit"` \| `"write"` \| `"delete"` \| `"move"` \| `"create"`. |
| `tool_call_id` | string | no | Correlation to the `tool_call` that caused it, if any. |
| `dest` | string | no | For `"move"`: the destination path. |

---

#### 4.2.7 `type: "approval"` — structured approval request

The structured reason behind `state: blocked`. Emitted when the agent needs
user input to proceed (permission, confirmation, free-form prompt). Carries
enough structure for the host to render buttons and, if it chooses, act on
them through the terminal's input channel.

```json
{
  "v": 1, "agent": "pi", "type": "approval",
  "seq": 13300, "ts": 1721234586000,
  "id": "apr-7",
  "kind": "permission",
  "prompt": "Allow bash to run `rm -rf target/`?",
  "options": ["yes", "no", "always"],
  "default": "no",
  "tool": "bash",
  "tool_call_id": "tc-44",
  "risk": "high",
  "timeout_ms": 0
}
```

| Field | Type | Required | Meaning |
|---|---|:---:|---|
| `id` | string | yes | Approval id. The host uses it to correlate a later `state` event (when the user answers) with this request. |
| `kind` | string | yes | `"permission"` \| `"confirm"` \| `"prompt"` \| `"select"` \| `"resolved"`. |
| `prompt` | string | yes | The question text. Cap 1024 chars. |
| `options` | array of string | no | For `"confirm"`/`"select"`: the choices. Defaults to `["yes","no"]` for `confirm`. |
| `default` | string | no | The default option (what the agent assumes if it times out). |
| `tool` | string | no | The tool requesting approval, if any. |
| `tool_call_id` | string | no | Correlation to the `tool_call` awaiting approval. |
| `risk` | string | no | `"low"` \| `"medium"` \| `"high"`. Host may color-code. |
| `timeout_ms` | integer | no | If the agent will auto-decide after this many ms; `0` = no timeout. |
| `choices` | array of object | no | For `"select"` with rich options: `[{"value":"a","label":"Option A"}, …]`. Overrides `options`. |

The agent SHOULD emit a `state: blocked` event alongside `approval` (or
shortly before). When the user answers (via the host injecting input, or by
typing in the terminal), the agent emits `state: working` (or `idle`/`error`)
and MAY emit a new `approval` with the same `id` and a `"resolved"` kind to
record the outcome.

### 4.3 Redaction

Agents frequently handle secrets: API keys in `bash` commands, file contents
in `read`/`write`, tokens in URLs. To keep OSC 9;7 safe to log and display:

1. **`args` on `tool_call` is a summary, not a raw payload.**
   The agent SHOULD truncate to a reasonable length (e.g. 256 chars) and
   redact obvious secrets (env vars matching `*(KEY|TOKEN|SECRET|PASSWORD)*`,
   long base64/hex strings, `Authorization` headers).
2. **`args_redacted` flag** signals that redaction happened, so the host can
   show a "redacted" indicator rather than implying the summary is
   complete.
3. **Never put credentials in any field.** The protocol carries status
   metadata only. If a field would require a secret, omit it.

Receivers SHOULD treat all free-text fields as untrusted display data and
avoid writing them to persistent logs at high verbosity.

---

## 5. State machine

When an agent emits OSC 9;7, the agent is the **state authority** for its
own lifecycle. The host trusts the reported `state` directly; it does not
second-guess with screen scanning for agents that emit OSC 9;7. (A host may
still fall back to screen-scanning for agents that do *not* emit OSC 9;7;
that is out of scope for this spec. The rule is only that the two must not
run simultaneously for the same agent, to avoid two competing sources of
truth.)

### 5.1 States

| State | Meaning | Suggested host badge |
|---|---|---|
| `working` | Agent is actively processing (LLM streaming, tool running). | 🟢 spinner |
| `blocked` | Agent is waiting for user input (approval/permission/prompt). | 🟠 pulsing + action buttons |
| `idle` | Agent turn finished, awaiting next prompt. | ⚪ dim |
| `done` | Agent session ended cleanly (process exit 0, or explicit done). | ✅ check |
| `error` | Agent hit a non-retryable error or crashed (exit ≠ 0). | ❌ red + message |

### 5.2 State-machine rules the agent should implement

These rules are the battle-tested part (they originate from agent-monitor
integrations that have shipped). An agent emitting OSC 9;7 **should**
replicate them to avoid flicker and false states:

1. **Idle debounce (250 ms).** On `agent_end`, do not emit `idle` immediately.
   Wait 250 ms; if a new `agent_start` arrives in that window, cancel the idle
   and emit `working`. This prevents flapping between turns.
2. **Retry grace (2.5 s).** On `agent_end` with a retryable provider error
   (rate limit / 429 / timeout / network / overloaded / …), emit `working`
   for 2.5 s, then emit `error` (or `blocked` if the agent re-prompted).
   Avoids a false `idle`/`error` while the agent auto-retries.
3. **State queue + in-flight coalescing.** Multiple rapid state changes are
   coalesced into one OSC 9;7 emit carrying the latest `state`/`seq`. Never
   emit two OSCs concurrently.
4. **Reload-safe.** On `session_start` with `reason: "reload"`, re-sync
   `agentActive = !isIdle()` and emit the current state. A reload may tear
   down and rebind the agent's extension runtime without ending the agent
   turn; missing this produces a stale `idle`.
5. **`seq` strictly increasing.** Use `Date.now() * 1000 + counter` or an
   atomic increment. The host drops events with `seq <= last_applied_seq`.
6. **`blocked` counting.** If the agent supports multiple concurrent
   blockers, keep a count; `blocked` while count > 0, else the underlying
   working/idle.
7. **No release needed.** When the agent process exits, the host detects it
   via the terminal's own PTY/SSH channel and marks the card `done`/`crashed`
   itself. The agent may emit a final `state: "done"` for a clean exit, but it
   is optional.

### 5.3 Staleness

The host tracks `last_event_ts` per (terminal, agent). If no OSC 9;7 arrives
for `stale_threshold_ms` (default 300 000 = 5 min) **and** the terminal
process is still alive, the card is marked `stale` (grey question-mark
badge). This catches agents that have frozen without emitting anything. If
the process is dead, the card is `done` or `crashed` (exit code from the
PTY), regardless of the last OSC. The threshold should be configurable.

### 5.4 Current OneTerm Agent Panel behavior

In OneTerm today, OSC 9;7 events are folded by `oneterm-state::AgentRegistry` and rendered by `crates/agent-ui` as a compact card list. The current panel shows:
- state badge + liveness summary
- optional model row and context bar
- the running tool row, or the most recent finished tool row when no tool is active
- age + session id in the footer

Cards are grouped by terminal tab and sorted by state priority (`blocked → error → working → stale → idle → done → ended`). Approval events are stored in the registry as `pending_approval` / `resolved_note`; the current card UI does not yet render inline approval buttons or a file feed.


## 6. Security considerations

1. **Size cap.** Mandatory 8 KiB cap on the base64 payload (§3.2). Without
   it, a malicious or buggy agent can flood the terminal with multi-megabyte
   OSCs and stall the parser.
2. **Malformed payloads.** Invalid base64 / UTF-8 / JSON / schema version /
   `type` MUST be dropped silently (§3.3). The terminal must never crash or
   render artefacts on a malformed agent OSC.
3. **One-directional.** OSC 9;7 carries no reply. A host that wants to act
   on an agent (approve, interrupt) does so through the terminal's input
   channel, not through OSC. This keeps the protocol simple and prevents an
   agent from spoofing host actions.
4. **Trust.** OSC 9;7 is emitted by the process running inside the terminal.
   The host already trusts that process with the terminal's input/output;
   OSC 9;7 adds no new trust boundary. The only new surface is the size cap
   above.
5. **Remote agents.** OSC 9;7 traverses SSH unchanged. A remote agent can
   report state to a local host exactly like a local agent. No credentials
   or secrets are involved; the payload is status metadata only.
6. **Path privacy.** `session.project_dir` can reveal local usernames, mount
   names, or repository names. Treat it as local UI metadata: sanitize before
   display and avoid forwarding it to external telemetry without explicit user
   consent.

---

## 7. Rationale: why base64 (not raw JSON)

This section records the investigation behind the §3.1 decision, kept for
future implementers who may be tempted to send raw JSON.

**Question.** Do VT engines split the OSC payload on `;` before the
application receives the parameters?

**Answer — yes.** Verified in a common vendored vte fork:

- In the OSC-string state, byte `0x3B` (`;`) calls a "put parameter" action
  that finalises the current parameter slice and starts a new one. Every
  other byte appends to the current parameter's raw buffer.
- The OSC dispatch routine rebuilds `params: &[&[u8]]` from the recorded
  `(start, end)` indices, one slice per parameter. So `params` is already
  `;`-split before dispatch runs.
- Downstream, each `&[u8]` slice may be copied into an owned `Vec<u8>` and
  passed to the application's OSC handler as a list of parameters.

**Conclusion.** The application cannot recover a `;` that was inside the
JSON payload: it has already been consumed as a parameter separator. The
existing OSC 9 notification code in many terminals works only because it
rejoins `params[1..]` with `;` *and* notification messages are free-form
text where re-joining is acceptable. JSON re-joining would silently corrupt
any value that contained `;` (a timestamp string, an error message, a URL
with query params).

**Decision — base64-wrap the whole JSON payload.** Single wire form, no
`{`-prefix detection, no raw-JSON fallback. ~33 % size overhead, acceptable
for status events.

---

## 8. Test plan (protocol conformance)

A conformance test suite for an OSC 9;7 receiver should cover:

1. **Valid payloads (all base64-wrapped per §3.1):**
   - One valid payload per event type: `state`, `session`, `heartbeat`,
     `model`, `tool_call` (start/update/end), `file`, `approval`.
   - JSON whose string value contains `;` — base64-wrapped, must decode and
     JSON-parse correctly (regression for §7).
2. **Malformed payloads → dropped silently (no crash, no artefact):**
   - Invalid base64 (non-base64 chars in the parameter).
   - Valid base64 but invalid UTF-8 after decode.
   - Valid base64 but invalid JSON after decode.
   - Unknown `v` (future schema version).
   - Unknown `type`.
   - Extra unknown envelope fields (must be ignored, not dropped).
   - Missing required fields for a given `type` (e.g. `state` without
     `state`) → dropped.
3. **Sequencing:**
   - `seq` dedup: an event with `seq <= last_applied_seq` is dropped.
   - `seq` strictly increasing accepted.
4. **Size cap:**
   - Payload at exactly 8 KiB base64 accepted.
   - Payload at 8 KiB + 1 dropped.
5. **Correlation:**
   - `tool_call` start → update → end share `tool_call_id`; host groups them.
   - `file.tool_call_id` resolves to a prior `tool_call`.
6. **Redaction:**
   - A `tool_call` with `args_redacted: true` is rendered with a redacted
     indicator and the raw `args` (even if present) is treated as a
     redacted summary, not the full input.
7. **Staleness:**
   - No event for `stale_threshold_ms` while the process is alive → `stale`.
   - A `heartbeat` with `interval_ms` resets the stale timer to
     `max(stale_threshold_ms, interval_ms * 3)`.
   - Process dead → `done`/`crashed` from the exit code, regardless of last
     OSC.
8. **Forward compatibility:**
   - A future-schema (`v: 2`) event is dropped silently.
   - An event of unknown `type` is dropped silently.
   - An event with extra fields in the envelope or in a known type is
     accepted; the extra fields are preserved or ignored.
