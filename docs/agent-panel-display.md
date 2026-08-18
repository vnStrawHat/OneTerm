# Agent Panel — current display implementation

> Companion to [`docs/osc-agent-status.md`](osc-agent-status.md).
> That document defines the OSC 9;7 wire protocol and typed payloads. This
> document describes what OneTerm currently stores and renders for the Agent
> Panel.
>
> Scope: **current implementation** in `crates/agent-ui`,
> `crates/state/src/agent_registry.rs`, and the `terminal-view` feed path. It is
> intentionally not a future UI roadmap.

---

## 1. Context

### 1.1 Where the panel lives

The Agent Panel is the right-dock panel used by Agent Mode. The concrete dock
panel is `crates/app/src/agent_panel.rs` (`panel_name = "agent_panel"`). It
hosts `oneterm_agent_ui::AgentListView`.

The display implementation lives in `crates/agent-ui`:

- `src/lib.rs` re-exports `AgentListView` and owns the feature `init()`.
- `src/view.rs` owns `AgentListView`: registry subscriptions, filters, the
  refresh task (spinner tick, relative-time refresh, stale refresh), and renders
  the panel header, filter chips, tab groups, group badges, the empty state, and
  the scroll column.
- `src/card.rs` renders compact agent cards.

`crates/agent-ui` does not parse OSC bytes and does not know terminal protocol
internals. It reads the folded display model from `oneterm_state::AgentRegistry`.

### 1.2 Implemented pieces

| Piece | Location | Current status |
|---|---|---|
| OSC 9;7 parse + envelope validation | `oneterm_terminal::osc_agent::parse_agent_status` | implemented |
| Typed event enum | `oneterm_terminal::osc_agent::AgentStatusEvent` | implemented |
| Per-`(terminal, agent)` `seq` dedup | `oneterm_terminal::osc_agent::should_apply` | implemented by backend listeners |
| Event delivery to terminal views | `SessionEvent::AgentStatus(Arc<AgentStatusEvent>)` | implemented |
| Registry fold | `oneterm_state::AgentRegistry` | implemented |
| Terminal feed into registry | `LocalTerminalView::push_agent_status` | implemented |
| Card navigation index + focuser | `crates/terminal-view/src/agent.rs` | implemented |
| Right-dock panel shell | `crates/app/src/agent_panel.rs` | implemented |
| Agent list UI | `crates/agent-ui` | implemented as compact cards |

---

## 2. Identity & grouping model

An agent reports OSC 9;7 into one terminal view. The panel renders one card per
`(terminal_key, agent_id)` and groups cards by the terminal tab that contains
that terminal view.

```text
Agent Panel
└─ Tab group           (one rendered group per tab_key that has visible cards)
   └─ Agent card       (one per (terminal_key, agent_id))
```

### 2.1 Keys

| Key | Type | Source | Current use |
|---|---|---|---|
| `terminal_key` | `gpui::EntityId` of `LocalTerminalView` | terminal view context | Card identity and click-to-focus target. |
| `agent_id` | `String` from envelope `agent` | OSC 9;7 event | Card identity. Multiple agents per terminal are supported by the model. |
| `tab_key` | `gpui::EntityId` of `TerminalPanel` | `SplitContext.panel` | Group cards into tab sections. |
| `tab_title` | `String` | terminal panel title helper | Rendered as the tab-group header. Renamed tabs update existing cards. |
| `space_label` | `String` | terminal panel space helper | Stored as `single` for one-space tabs, or `#N` for split tabs. The card renders `single` as `#0`. |
| `space_active` | `bool` | `SplitContext` / terminal panel state | Stored in the card. It is not currently highlighted by `agent-ui`. |

`terminal-view` is the only crate that knows the tab/space hierarchy. It pushes
this grouping metadata into `AgentRegistry` together with each parsed
`AgentStatusEvent`.

---

## 3. Backing model

### 3.1 `AgentRegistry`

`AgentRegistry` is a global `Entity<AgentRegistry>` in `oneterm-state`. It stores
cards in insertion order and exposes:

- `apply(terminal_key, grouping, ev, cx)` — create or update the matching card,
  refresh grouping metadata, fold the event, and notify observers.
- `rename_tab_title(tab_key, tab_title, cx)` — update the group title for cards
  in a tab.
- `set_lifecycle(terminal_key, lifecycle, cx)` — mark cards from a terminal as
  `Live`, `Stale`, or `Ended`.
- `remove_terminal(terminal_key, cx)` — drop cards for a terminal that was truly
  closed.
- `clear_ended(cx)` — remove ended cards from the registry. There is no current
  `agent-ui` button wired to this method.
- `refresh_stale(cx)` — mark live cards stale when no event has arrived within
  the effective stale window.
- `summary()` — aggregate counts for the header chips. `Lifecycle::Ended` counts
  as done.

The parser/listener applies `seq` dedup before events reach the registry. The
registry still stores `last_seq` as defensive/debug state.

### 3.2 `AgentCard`

`AgentCard` is the folded per-agent display model. It stores:

- identity/grouping: `terminal_key`, `agent_id`, `tab_key`, `tab_title`,
  `space_label`, `space_active`
- lifecycle/session: `state`, `message`, `session_id`, `session_reason`,
  `parent_id`, `project_dir`
- liveness: `last_event_ts`, `last_recv`, `heartbeat_interval`, `lifecycle`
- model/context: `model`, `context_used`
- tool activity: `current_tool`, `recent_tools` (last 8)
- file activity: `recent_files` (last 6)
- approvals: `pending_approval`, `resolved_note`
- debug: `last_seq`

The current card UI uses only a subset: state/liveness, model/context, project
directory, current or last tool, age, session id, and click navigation. File
activity, approval state, `session_reason`, `parent_id`, and `resolved_note` are
folded but not rendered yet.

### 3.3 Fold rules

| Event `type` | Current registry effect |
|---|---|
| `state` | Set `state`, `message`, and optional `session_id`. Clear `pending_approval` when the state is not `blocked`. If `state == done`, set lifecycle to `Ended { exit_code: None }` unless already ended. |
| `session` | Set `session_id`, `session_reason`, `parent_id`, and `project_dir`. |
| `heartbeat` | Set `heartbeat_interval`; if `state` is present, refresh `state`. |
| `model` | Replace `model`; update `context_used` only when present. Latest value wins. |
| `tool_call` / `start` | Set `current_tool` from `tool_call_id`, `tool`, `target`, `args`, `args_redacted`, `progress`, and start timestamp. |
| `tool_call` / `update` | If the update id matches `current_tool.id`, update `current_tool.progress`. |
| `tool_call` / `end` | Finalize the matching `current_tool`, or synthesize a finished run if no matching start is active. Push the result into `recent_tools`. |
| `file` | Push a file entry into `recent_files`, deduplicating consecutive identical entries. |
| `approval` / `resolved` | Set `resolved_note` from `default` or `"resolved"`, and clear `pending_approval`. |
| `approval` / other kinds | Set `pending_approval` and clear `resolved_note`. |

Every accepted event updates `last_event_ts`, `last_recv`, and `last_seq`. A
fresh event moves a non-ended card back to `Lifecycle::Live`.

### 3.4 Display sanitation

Free-text fields are sanitized in the registry before display: control
characters are replaced, values are trimmed to a single line, and per-field char
caps are applied. Important caps include: message 256, prompt 1024, args 200,
path 256, id 64, progress 120, error 256, target 160, provider 48, model 96.

---

## 4. Layout & information architecture

The current panel is a simple vertical view:

```text
┌─ Agents ───────────────────────────────┐
│ [# All] [⠋ Work] [▲ Block] [✕ Err] ... │
├────────────────────────────────────────┤
│ tab title                         ● 2  │
│ ┌ agent card ────────────────────────┐ │
│ │ state + agent/#space/live          │ │
│ │ provider > model [reasoning]       │ │
│ │ 84.5k / 200k  42%     project_dir │ │
│ │ current tool, or last finished tool│ │
│ │ 3s ago                         sid │ │
│ └────────────────────────────────────┘ │
└────────────────────────────────────────┘
```

Current behavior:

1. Empty registry: show the centered empty state `No agents reporting` and
   `Agents that emit OSC 9;7 appear here.`
2. Non-empty registry: show header, filter chips, and a scrolling list.
3. Tab groups are rendered for groups with at least one card passing the current
   filter.
4. Group order is the order in which each `tab_key` first appears in the current
   registry card snapshot. It is not currently recomputed from dock tab order.
5. Group headers show the tab title and aggregate status badges for working,
   blocked, error, and resting (`idle`/`done`/`ended`).
6. Groups are always expanded. There is no current collapse/expand UI.
7. Cards inside a group are sorted by `sort_rank()`, then by most recent
   `last_recv`.

Current card sort order:

```text
blocked → error → working → stale → idle → done → ended
```

`Lifecycle::Ended` cards are sorted last by the model, but the current view
filters ended cards out before rendering.

---

## 5. Card content

### 5.1 Card header

The header is a single compact row:

```text
<state indicator + state label>        <agent_id> <space_label>: <liveness>
```

- `working` uses a spinner frame.
- Other states use a colored dot.
- State labels are `working`, `blocked`, `idle`, `done`, and `error`.
- The right side is `agent_id`, rendered `space_label`, and `live` / `stale` /
  `ended` from `Lifecycle`.
- `space_label == "single"` renders as `#0`; split-space labels such as `#1`
  render unchanged.

Clicking the card focuses the terminal that produced the agent event.

### 5.2 Model row and context bar

If a model event has been folded, the card shows:

```text
<provider> > <model_name or model_id> [reasoning]
```

`reasoning` is shown as a small pill when true.

If `context_used` is present:

- with `context_window > 0`: show a thin bar and an info row. The info row
  places `<used> / <window>  <percent>%` on the left and `project_dir` on the
  right when available.
- without a usable `context_window`: show `<used> tokens` on the left and
  `project_dir` on the right when available.

`project_dir` is formatted for compact display only when it exceeds 45 chars,
keeping the root/first useful segment and the last two path segments, for example
`D:\TrungKFC-Research/.../Rust/myTerm2` or `/opt/.../dev/myProject`.

The current bar color is a hue ramp computed from the usage fraction. It is not
currently a discrete success/warning/danger threshold.

### 5.3 Activity row

If `current_tool` exists, the card shows a running row:

```text
<loader> <tool> <target or args> [redacted] [progress]
```

Details:

- The displayed detail is `target` if present, otherwise `args`.
- If the detail comes from `target`, the row uses start ellipsis so path/command
  tails remain visible.
- If the detail comes from `args`, the row uses normal end ellipsis.
- `args_redacted` adds a `redacted` chip.
- `progress` is shown at the end when present.

If there is no running tool and `recent_tools` is non-empty, the card shows the
last finished tool:

```text
<✓ or ✕> <tool> <target or args> [duration] [exit N]
```

The result row uses success color for non-error runs and danger color for error
runs. The current compact UI does not render `error_message` or `diff_stat`.

### 5.4 Footer

The footer shows:

```text
<age since last received event>                         sid <session_id>
```

The age is derived from host time (`last_recv.elapsed()`), not from the agent
clock. The session id is shown right-aligned when present and is ellipsized by
layout if needed.

### 5.5 Folded but not rendered

The registry currently stores but the compact card does not render:

- `recent_files`
- `pending_approval`
- `resolved_note`
- `session_reason`
- `parent_id`
- full `recent_tools` history beyond the last finished tool
- `error_message` and `diff_stat`

---

## 6. State → visual mapping

Colors are captured from the active gpui-component theme. The current palette
uses:

| Purpose | Current token |
|---|---|
| working / success | `success` |
| blocked / warning | `warning` |
| error | `danger` |
| done | `info` |
| idle / ended text | `muted_foreground` |
| normal text | `foreground` |
| card background | state accent with low opacity |
| group/header background | `tokens.tab_bar` |
| border | `border` |

The left and right card borders use the state accent. Ended cards are dimmed if
they are rendered by a future view, but the current `AgentListView` filters them
out.

---

## 7. Interaction model

OSC 9;7 is one-directional: agent → terminal host. The Agent Panel never replies
over OSC.

Current implemented interaction:

- Clicking a card calls `oneterm_state::agent_focus::focus_terminal` with the
  card's `terminal_key`.
- `terminal-view` registers the focuser. It activates the tab, activates the
  space, and focuses the terminal using OneTerm dock/space/focus APIs.

Not currently implemented in `agent-ui`:

- approval buttons
- hover-to-highlight target space
- expanded card view
- copy actions
- group collapse/expand
- settings/gear menu actions

---

## 8. Header, summary, and filters

The header title is `Agents` with a bot icon. The filter row always contains:

| Label | Filter |
|---|---|
| `All` | all non-ended cards |
| `Work` | `state == working` |
| `Block` | `state == blocked` |
| `Err` | `state == error` |
| `Idle` | `state == idle` |
| `Done` | `state == done` |

All filters currently exclude `Lifecycle::Ended` cards. The summary counts are
computed from the registry before filtering; `Lifecycle::Ended` contributes to
the done count.

Group badges show counts for:

- working
- blocked
- error
- resting (`idle`, `done`, and `ended`)

---

## 9. Lifecycle & liveness

`AgentCard.lifecycle` is host-derived and independent of the agent-reported
`state`:

| Lifecycle | Source | Current effect |
|---|---|---|
| `Live` | any fresh non-ended event | card renders normally |
| `Stale` | registry stale refresh | liveness text changes to `stale`; sort rank moves after working |
| `Ended` | terminal `Exited`/`Closed`, or `state: done` | registry retains the state, summary counts as done, current view hides the card |

Stale detection uses `UiConfig::agent_stale_threshold_ms()` with a default of
300,000 ms. A heartbeat with `interval_ms` raises the effective threshold to at
least `interval_ms * 3`.

`AgentListView` runs one 120 ms tick (`ACTIVE_CARD_TICK`, only notifies while a
visible card is working, for the spinner), refreshes relative-time labels every
1 s (`RELATIVE_TIME_TICK`) and runs stale refresh every 15 s (`STALE_TICK`) —
`crates/agent-ui/src/view.rs`.

---

## 10. Security & redaction display rules

The parser already enforces the OSC payload size cap and schema validation from
[`docs/osc-agent-status.md`](osc-agent-status.md). The display layer still treats
all text as untrusted:

1. Registry sanitation strips control characters and caps display strings.
2. `args_redacted` is rendered as a `redacted` chip.
3. Tool `args` are treated as summaries, not as full raw inputs.
4. The panel never feeds OSC text back into a terminal output path.
5. The panel sends user interaction only through OneTerm focus/navigation APIs;
   it does not add an OSC reply channel.

---

## 11. Theming, sizing, and performance

- The UI uses theme tokens; no component hardcodes status colors.
- Cards are compact and width-tolerant.
- Long text is clipped with GPUI ellipsis behavior.
- Tool targets use start ellipsis so the tail stays visible; tool args use end
  ellipsis.
- The list is a simple scrolling column (`overflow_y_scroll`). It is not
  virtualized.
- Registry changes call `cx.notify()`. The view also refreshes periodically for
  animation and relative time.

---

## 12. Data flow

```text
agent stdout
  ESC ] 9 ; 7 ; <base64-json> BEL
        │
backend listener (local-shell / ssh)
        │  parse_agent_status() + should_apply()
        ▼
SessionEvent::AgentStatus(Arc<AgentStatusEvent>)
        │
LocalTerminalView
        │  push_agent_status(ev)
        │  - resolve Tab/Space grouping
        │  - register navigation target
        │  - AgentRegistry::apply(...)
        │  agent_status = Some(ev)  // latest-event stash
        ▼
oneterm_state::AgentRegistry
        │  folds events into AgentCard values
        │  notifies observers
        ▼
oneterm_agent_ui::AgentListView
        │  groups, filters, sorts, renders compact cards
        ▼
crates/app::AgentPanel
```

Terminal lifecycle events also feed the registry:

- `SessionEvent::Exited(code)` → `Lifecycle::Ended { exit_code: Some(code) }`
- `SessionEvent::Closed` → `Lifecycle::Ended { exit_code: None }`
- true terminal close/drop → `remove_terminal(terminal_key)` and remove navigation
  entry

---

## 13. Current gaps / not rendered yet

The registry already folds more information than the compact panel displays.
These are known current gaps, not active behavior:

- Inline approval banner and answer buttons are not rendered.
- `recent_files` is not rendered.
- Expanded cards and full tool history are not rendered.
- `error_message` and `diff_stat` are folded but not shown.
- `session_reason` and `parent_id` are folded but not shown.
- Tab groups are not collapsible.
- There is no Agent Panel settings/gear menu.
- Ended cards are retained by the registry but hidden by the current view.
- Group order is first-seen registry order, not live dock tab order.

---

## 14. Decisions reflected by the current code

### 14.1 Space identity and ordering

Agent cards display `#N`, where `N` is the terminal Space's raw, stable `SpaceId`.
`SpaceId(0)` identifies the tab's initial terminal, while split-created Spaces use
monotonically increasing IDs. Card ordering is a separate concern: `space_order`
is the Space's current 0-based depth-first position and is used only to sort cards
within a tab. Closing a Space can change ordering, but never renumbers surviving
card labels.

### 14.2 Multiple agents per terminal

Legal but rare. The registry key is `(terminal_key, agent_id)`, so multiple
agent ids emitted from one terminal become separate cards in the same tab group.

### 14.3 Stale threshold

The stale threshold is stored in `UiConfig` as `agent_stale_threshold_ms` and
falls back to 300,000 ms. Per-card heartbeat cadence can raise the effective
threshold to `max(configured_threshold, heartbeat_interval * 3)`.

### 14.4 Ended-card handling

Terminal close/exit is host-authoritative. The registry records ended state and
counts ended cards as done, but the current `AgentListView` excludes ended cards
from rendering. A true terminal close removes the card from the registry.

### 14.5 Click behavior

Clicking a card activates the associated terminal via OneTerm's tab/space/focus
APIs. It never sends an OSC reply.
