# Agent Panel — display design

> Companion to [`docs/osc-agent-status.md`](osc-agent-status.md). That document
> defines the **wire protocol** (OSC 9;7 → typed `AgentStatusEvent`). This
> document defines **what the host renders**: the content, layout, grouping,
> state model, and interaction of OneTerm's **Agent Panel** — the right-dock
> "fleet view" of coding agents running inside terminals.
>
> Scope: **display content + backing model**. It does not redefine the
> protocol, the parser, or the seq/dedup rules (those are done — see §1.2).

---

## 1. Context

### 1.1 Where the panel lives

The Agent Panel is OneTerm's right dock in **Agent Mode**
(`oneterm_core::RightDockMode::Agent`). Today `crates/app/src/agent_panel.rs`
registers a `DockItem::Panel` named `"agent_panel"` that renders a
"coming soon" placeholder. This spec defines the real content that replaces it.

Following the established composite-panel pattern (`SshClientPanel` composes
`SessionPanel` + `SftpPanel`), the Agent Panel's **display logic lives in the
`agent-ui` feature crate** and the `app` crate composes it into the dock panel.
This obeys the crate rules (R5/R9): a feature crate depends only on the shared
layers; only `app` may know more than one feature.

```
oneterm-app (agent_panel.rs, DockItem::Panel "agent_panel")
   └─ composes ─► oneterm-agent-ui (AgentListView + AgentCardView)   ← feature crate
                     └─ reads ─► oneterm-state::AgentRegistry (global Entity)
                                    ▲ fed by
oneterm-terminal-view (LocalTerminalView) ── SessionEvent::AgentStatus ──┘
```

### 1.2 What already exists (do not rebuild)

| Piece | Location | Status |
|---|---|---|
| OSC 9;7 parse + envelope validation | `oneterm-terminal::osc_agent::parse_agent_status` | ✅ done |
| Typed event enum | `oneterm-terminal::osc_agent::AgentStatusEvent` (7 variants) | ✅ done |
| Per-`(terminal, agent)` `seq` dedup | `oneterm-terminal::osc_agent::should_apply` | ✅ done (applied in backend listeners) |
| Event delivery to the UI | `SessionEvent::AgentStatus(Arc<AgentStatusEvent>)` | ✅ done |
| Latest-event stash on the view | `LocalTerminalView.agent_status` | ✅ done (latest event only) |
| Right-dock Agent mode toggle | `RightDockMode::Agent` → panel `"agent_panel"` | ✅ wired |
| Panel placeholder | `app/src/agent_panel.rs` | 🔸 to be replaced by this spec |
| Aggregated display model + panel content | `agent-ui`, `state::AgentRegistry` | ❌ this spec |

### 1.3 The core gap this spec closes

`LocalTerminalView.agent_status` keeps **only the latest event**. A useful
panel needs the *folded* history: the current lifecycle state **and** the
active model, context usage, the running/last tool call, a recent-file feed,
and any pending approval — each of which arrives as a **different** event
`type`. So the design introduces a **backing model** (`AgentCard`) that folds
the event stream, held in a global **`AgentRegistry`** the panel observes.

---

## 2. Identity & grouping model

The explicit requirement: **a Tab may host one or many agents, and multiple
agents live in different Spaces within a Tab.** OneTerm's tab/space hierarchy
maps directly:

```
TerminalPanel  (= a Tab in the center DockArea)
└─ SpaceTree
   ├─ SpaceLeaf(SpaceId) ─ SpaceContent::Terminal(Entity<LocalTerminalView>)   ← may run an agent
   ├─ SpaceLeaf(SpaceId) ─ SpaceContent::Terminal(Entity<LocalTerminalView>)   ← may run an agent
   └─ SpaceLeaf(SpaceId) ─ SpaceContent::Empty
```

An agent reports OSC 9;7 into the terminal of exactly **one Space**. Therefore
the display hierarchy is:

```
Agent Panel
└─ Tab group           (one per TerminalPanel that has ≥1 agent-bearing Space)
   └─ Agent card       (one per (Space, agent_id) that has reported OSC 9;7)
```

### 2.1 Keys

| Key | Type | Source | Why |
|---|---|---|---|
| `terminal_key` | `gpui::EntityId` of the `LocalTerminalView` | the view itself | Globally unique, stable for the terminal's lifetime. Natural per-terminal multiplexing (OSC 9;7 §1.1.3). |
| `agent_id` | `String` (envelope `agent`) | event envelope | A single terminal *may* host >1 agent (dedup is per `(terminal, agent)`); the card key is the pair. Normally 1 agent/terminal. |
| `tab_key` | `gpui::EntityId` of the `TerminalPanel` | `SplitContext.panel` | Group cards by Tab. |
| `space_id` | `SpaceId` | `SplitContext.space_id` | Label the card with its Space and support click-to-focus. |

> **Card identity = `(terminal_key, agent_id)`.** Grouping key = `tab_key`.
> The `terminal-view` crate is the only place that knows the tab/space, so it
> pushes the grouping metadata (`tab_key`, `tab_title`, `space_label`,
> `space_active`) into the registry alongside each event. `state` therefore
> stays feature-agnostic (no dependency on `terminal-view`).

---

## 3. Backing model (what feeds the display)

### 3.1 `AgentRegistry` (global, in `oneterm-state`)

A global `Entity<AgentRegistry>` (registered like `AppState`). `state` already
depends on `oneterm-terminal` (for `AgentStatusEvent`) and on
`gpui`/`gpui-component`, so this adds no new edge and keeps the panel
feature-agnostic.

```
AgentRegistry {
    cards: IndexMap<(EntityId, String), AgentCard>,   // insertion-ordered
    stale_threshold: Duration,                        // from UiConfig.agent_stale_threshold_ms (default 300_000)
}
```

API (sketch — final signatures live in code):

- `apply(&mut self, key, grouping: Grouping, ev: &AgentStatusEvent, cx)` —
  fold one event into the matching `AgentCard` (create if absent), then
  `cx.notify()`. `seq` dedup is already done upstream; the registry may still
  guard with `card.last_seq`.
- `set_lifecycle(&mut self, key, Lifecycle, cx)` — the view reports terminal
  exit/close so the card becomes `Ended{exit_code}` (protocol §5.2.7: the host,
  not the agent, is authoritative for process death).
- `remove_terminal(&mut self, terminal_key, cx)` — drop all cards for a
  terminal that was closed/dragged away.
- `refresh_stale(&mut self, now, cx)` — a low-frequency tick marks cards
  `stale` (protocol §5.3).
- Read side: `groups()` / `cards_for_tab(tab_key)` for the panel.

### 3.2 `AgentCard` — the folded per-agent state

```
AgentCard {
    // ── identity / grouping ──
    terminal_key: EntityId,
    agent_id: String,
    tab_key: EntityId,
    tab_title: String,
    space_label: String,      // "single", or "#N" (tree order) for a split tab
    space_active: bool,       // this Space is the focused one

    // ── lifecycle (from `state` / `session`) ──
    state: AgentState,        // working | blocked | idle | done | error
    message: Option<String>,  // ≤256 chars, single-line
    session_id: Option<String>,
    session_reason: Option<String>,   // startup|reload|new|resume|fork
    parent_id: Option<String>,        // fork lineage

    // ── liveness (from `heartbeat` + local clock) ──
    last_event_ts: u64,               // agent clock (ms) — display "N ago"
    last_recv: Instant,               // host clock — stale detection
    heartbeat_interval: Option<u64>,  // tighten stale timer to max(threshold, 3×interval)
    lifecycle: Live | Stale | Ended { exit_code: Option<i32> },

    // ── model / context (from `model`) ──
    model: Option<ModelInfo>,         // provider, model_id, model_name, ctx_window, max_out, reasoning
    context_used: Option<u64>,        // latest wins → usage bar

    // ── activity (from `tool_call`) ──
    current_tool: Option<ToolRun>,    // start seen, no end yet → spinner row
    recent_tools: RingBuffer<ToolRun, 8>,   // most-recent ended calls

    // ── files (from `file`) ──
    recent_files: RingBuffer<FileEntry, 6>,

    // ── approval (from `approval`) ──
    pending_approval: Option<ApprovalInfo>,   // reason for `blocked`; cleared on `resolved`/unblock

    // ── debug ──
    last_seq: u64,
}
```

### 3.3 Fold rules — event `type` → card mutation

| Event `type` | Effect on the `AgentCard` |
|---|---|
| `state` | Set `state`, `message`, `session_id`. If `state != blocked`, clear `pending_approval`. If `state == done`, mark `lifecycle = Ended{None}` unless already ended. |
| `session` | Set `session_id`, `session_reason`, `parent_id`. Used for the session chip / "forked from" hint. |
| `heartbeat` | Update `last_event_ts`/`last_recv`, `heartbeat_interval`. If `state` present, refresh `state` (low-priority). Never downgrades a fresher `seq`. |
| `model` | Set/replace `model`; latest `context_used` **wins** (drives the usage bar). |
| `tool_call` phase=`start` | `current_tool = ToolRun{ id, tool, target, args, args_redacted, started }`. |
| `tool_call` phase=`update` | If ids match, set `current_tool.progress`. |
| `tool_call` phase=`end` | Finalise `current_tool` (exit_code, is_error, error_message, duration_ms, diff_stat) → push to `recent_tools`; clear `current_tool`. |
| `file` | Push `{ path, action, dest?, tool_call_id? }` to `recent_files` (dedup consecutive identical). |
| `approval` kind∈{permission,confirm,prompt,select} | `pending_approval = ApprovalInfo{…}`; expect an accompanying `state: blocked`. |
| `approval` kind=`resolved` | Clear `pending_approval` (record outcome text for a brief "resolved: <opt>" note). |

Every fold refreshes `last_event_ts`/`last_recv` and `last_seq`.

---

## 4. Layout & information architecture

The panel is a vertically scrolling column. Wireframe (compact cards):

```
┌─ Agents ───────────────────────────  ⚙ ─┐   header: title + settings
│ 2 working · 1 blocked · 3 idle · 1 err   │   summary counts
│ [All] [Working] [Blocked] [Errors]       │   filter chips
├──────────────────────────────────────────┤
│ ▾ prod-api                     🟢2 🟠1    │   Tab group header (collapsible) + aggregate badges
│ ┌────────────────────────────────────────┐│
│ │ 🟢 pi              #1          ● live   ││   card header: badge · agent · space · liveness dot
│ │ Claude Sonnet 4 · anthropic · reasoning ││   model chip
│ │ ▓▓▓▓▓▓▓░░░  84.5k / 200k  42%           ││   context-usage bar
│ │ ▸ bash  grep -n TODO src/app.rs         ││   running tool (spinner) / last tool
│ │ ~ src/app.rs  edit  +12 −3              ││   recent file
│ │ 3s ago · sid abc12…                     ││   footer: relative time + session id
│ └────────────────────────────────────────┘│
│ ┌────────────────────────────────────────┐│
│ │ 🟠 codex           #2          ● live   ││
│ │ ⚠ high  Allow bash to run `rm -rf tgt`? ││   approval banner (blocked → expanded)
│ │   [ yes ]  [ no* ]  [ always ]          ││   * = default
│ └────────────────────────────────────────┘│
│ ▸ local                             ⚪1    │   collapsed tab group (idle only)
└──────────────────────────────────────────┘
```

Rules:

1. **Tab group** = one section per `TerminalPanel` with ≥1 card. Header shows
   the tab title + aggregate state badges (counts per state). Collapsible;
   collapsed by default when the group has only idle/done cards.
2. **Card ordering within a group**: `blocked` → `error` → `working` →
   `stale` → `idle` → `done/ended`; ties broken by most-recent `last_recv`.
   Groups ordered by the center dock's tab order; the group containing the
   active Space is highlighted (not necessarily reordered).
3. **Card density**: compact by default. A card auto-expands its approval
   banner when `blocked`, and shows the error `message` when `error`. Clicking
   a card toggles an expanded view (full tool history + file list + session
   lineage).
4. **Empty state**: when `cards` is empty, center a hint (mirrors the current
   placeholder but explains the mechanism): an agent glyph + "No agents
   reporting" + "Agents that emit OSC 9;7 appear here."

---

## 5. Card content — field-by-field

### 5.1 Card header
`<state badge>  <agent_id>   <space_label>   <liveness dot>`

- **State badge**: emoji per protocol §5.1 (`AgentState::badge()` already
  returns these) *or* a themed dot (see §6). Prefer a themed dot + short label
  for a native look; keep the emoji as an accessible fallback.
- **agent_id**: bold, e.g. `pi`, `codex`, `claude`. Used for an icon/label the
  host may map to a per-agent glyph later.
- **space_label**: muted; `single` when the tab has one Space, else `#N` — the
  agent's Space numbered in `SpaceTree` order (depth-first, left→right). This
  label is stable and unambiguous under nested splits; the *positional* Space
  is conveyed instead by the hover highlight + the active-Space accent
  (see §7), not by the label text.
- **liveness dot**: `● live` (success), `◐ stale` (muted/warning), `○ ended`
  (muted); tooltip shows `last_event_ts` relative time and exit code if ended.

### 5.2 Model chip + context bar (from `model`)
- Line: `<model_name or model_id> · <provider>` + a `reasoning` pill when true.
- **Context-usage bar**: `context_used / context_window` as a filled bar +
  label `84.5k / 200k  42%`. Bar fill color ramps by fraction: `success`
  < 70% → `warning` 70–90% → `danger` ≥ 90% (all theme tokens, §6). Hidden
  when `context_window` is absent; shows just `context_used` tokens if only
  that is known. "Latest `context_used` wins" (protocol §4.2.4).

### 5.3 Activity row (from `tool_call`)
- **Running** (`current_tool` set): a spinner + `<tool>  <target or args>` +
  live `progress` if provided. `args` shown as a single truncated line; if
  `args_redacted`, append a muted `redacted` chip and treat `args` as a summary
  only (never the full input — protocol §4.3).
- **Last finished**: `<tool>  <target>  <duration> <✓|✗>`. `✗` uses `danger`
  when `is_error`; show `error_message` (truncated) beneath. `diff_stat`
  (`+N −M`) shown for edit/write tools.
- The expanded card lists `recent_tools` (last 8).

### 5.4 File feed (from `file`)
Compact list of `recent_files` (last 6): `<action-icon> <path>` with a muted
`action` word; for `move`, show `path → dest`. Action icons:
`read` eye · `edit` pencil · `write`/`create` file-plus · `delete` trash ·
`move` arrow-right. Independent of tool calls (protocol §4.2.6), so it works
even if the agent emits only `file` events.

### 5.5 Approval banner (from `approval`, when `blocked`)
The reason behind `state: blocked`. Rendered as a colored banner:

- Risk pill (`low`/`medium`/`high` → `info`/`warning`/`danger`).
- `prompt` (≤1024 chars, wrapped, escaped).
- Option buttons from `options` (or `choices` for rich `select`, or default
  `[yes] [no]` for `confirm`); the `default` option marked (`no*`).
- If `timeout_ms > 0`, a countdown hint ("auto <default> in Ns").
- See §7 for whether buttons act or are display-only.

### 5.6 Footer
`<relative time since last_event_ts> · sid <session_id truncated>`.
Expanded card adds `parent_id` ("forked from …"), `session_reason`, and (debug
builds) `seq`.

---

## 6. State → visual mapping (theme tokens only)

No hardcoded colors — read from `cx.theme()`. Available tokens (verified in
`gpui-component` `ThemeColor`): `success`/`success_foreground`,
`warning`/`warning_foreground`, `danger`/`danger_foreground`, `info`,
`accent`, `muted_foreground`, `foreground`, `background`, `border`, and the
`tokens.tab_bar` used by section headers (matches `SshClientPanel`).

| State | Accent token | Dot / badge | Card treatment |
|---|---|---|---|
| `working` | `success` | filled dot + spinner | normal; activity row live |
| `blocked` | `warning` | pulsing dot | approval banner expanded; **pinned to top** |
| `idle` | `muted_foreground` | dim hollow dot | collapsed detail |
| `done` | `muted_foreground` | check | collapsed; moved to bottom |
| `error` | `danger` | ✗ | `message` shown; **pinned to top** |
| `stale` (derived) | `warning`→`muted` | ◐ "?" | "stale" chip; detail dimmed |
| `ended` (derived) | `muted_foreground` | ○ | exit code chip (`exit 0` success / `exit N` danger) |

Left border accent (2px) of each card = the state accent token, giving a quick
scannable status column.

---

## 7. Interaction model

The protocol is **one-directional** (agent → host; OSC 9;7 §6.3). The host
never replies over OSC. Interactions therefore fall in two tiers:

**Tier 1 — monitor + navigate (v1, always safe):**
- **Click a card → activate the agent's terminal.** This is the primary
  navigation action. The card carries `(tab_key, space_id)`, so the target is
  unambiguous:
  1. Select the agent's **Tab** — make its `TerminalPanel` the active tab in
     the center `DockArea` (revealing it if the tab strip scrolled it off).
  2. **If the Tab has more than one Space**, also activate the agent's
     **Space** — `SpaceTree::set_active(space_id)` so the correct split leaf is
     focused and highlighted (the active-Space accent).
  3. Focus the `LocalTerminalView` so keystrokes go to that terminal.

  A single-Space tab uses steps 1 + 3 only ("activate the Tab"); a split tab
  uses all three ("activate Tab + Space"). This uses OneTerm's own
  dock / `SpaceTree::set_active` / focus APIs — **never** OSC (the protocol is
  one-directional). After activation the panel also refreshes each card's
  `space_active` flag so the just-focused card shows the active-Space accent.
- **Hover a card** → briefly highlight the target Space in its Tab (a soft
  outline on the `SpaceLeaf`) so the user can see *where* the agent runs before
  clicking. No focus change on hover.
- **Expand/collapse** a card or a tab group.
- **Copy** session id (and, on the expanded card, tool `args`/file paths).
- **Filter** by state; **collapse idle/done** toggle.

**Tier 2 — act via the terminal input channel (opt-in, later phase):**
- Approval buttons *may* send the chosen answer **through the terminal's own
  input channel** (`TerminalSession::send_text` / `send_keystroke`), never
  through OSC. This must be explicit and clearly the terminal's input, so an
  agent cannot be spoofed by the panel (protocol §6.3).
- **v1 default: buttons are display-only** ("waiting for your input in the
  terminal"), because the exact keystroke to answer a prompt is
  agent-specific. Wiring buttons to `send_text` is gated behind a per-agent
  opt-in and is out of scope for the first cut.

---

## 8. Header, summary, filters

- **Title**: "Agents".
- **Summary line**: live counts, e.g. `2 working · 1 blocked · 3 idle · 1 err`.
  Zero-count segments omitted. `blocked`/`error` counts colored.
- **Filter chips**: `All` · `Working` · `Blocked` · `Errors` (+ `Idle` when
  present). A chip filters the visible cards; group headers hide when empty.
- **Overflow menu (⚙)**: "Collapse idle/done", "Clear ended", the stale
  threshold (`UiConfig.agent_stale_threshold_ms`, default 300 000 ms), and a
  link to `docs/osc-agent-status.md`.

---

## 9. Lifecycle & liveness display

Driven by `AgentCard.lifecycle` (protocol §5.2.7 / §5.3):

- **Live**: recent event within the stale window. Normal render.
- **Stale**: no OSC 9;7 for `max(stale_threshold, 3 × heartbeat_interval)`
  while the terminal process is alive → grey "?" + "stale N ago". Set by the
  registry's periodic `refresh_stale`.
- **Ended**: the view reports `SessionEvent::Exited/Closed` →
  `Ended{exit_code}`. `exit 0` (or a final `state: done`) → ✅ done;
  `exit ≠ 0` → ❌ crashed. Ended cards drop to the bottom, greyed, and are
  removed on "Clear ended" or when the Tab/Space closes
  (`remove_terminal`).

When a Space is dragged into another Tab (OneTerm move semantics), the card's
grouping metadata is re-pushed on the next event, so it re-homes to the new
tab group automatically; a `remove_terminal` on true close prevents ghosts.

---

## 10. Security & redaction display rules

Consumes protocol §3.2 / §3.3 / §4.3 (already enforced upstream), plus
display-side hardening:

1. **Treat every free-text field as untrusted display data.** Render
   single-line, width-truncated, with control characters stripped (never feed
   `message`/`prompt`/`args`/`path` back through a VT path).
2. **Respect `args_redacted`**: show a "redacted" indicator; the `args` string
   is a summary, not the full command.
3. **Never persist** free-text fields at high verbosity (log at debug only).
4. Oversized / malformed events are already dropped by the parser (§3.2/§3.3),
   so the panel only ever sees validated `AgentStatusEvent`s.
5. The panel adds **no new trust boundary**: it displays what the in-terminal
   process already controls (protocol §6.4).

---

## 11. Theming, icons, sizing, performance

- **Colors**: all from `cx.theme()` (see §6). No literals.
- **Icons**: prefer existing `gpui_component::IconName` (Lucide) for tool/file
  glyphs; add OneTerm SVGs under `crates/theme/assets/icons/` only if a needed
  glyph is missing (auto-generates `AppIcon::<Name>`).
- **Sizing**: card min-width tolerant of a narrow dock; text truncates with
  ellipsis; the whole column scrolls; long tab groups virtualize if the card
  count is large.
- **Redraw**: the registry `cx.notify()`s on each applied event; the panel
  re-renders on notify. Coalesce is inherited from OSC 9;7's in-flight
  coalescing (protocol §5.2.3), so the panel does not need its own debounce for
  correctness — but a light animation frame budget for the spinner/pulse is
  fine.

---

## 12. Data flow (end to end)

```
agent → stdout:  ESC ] 9 ; 7 ; <b64(json)> BEL
        │
backend listener (ssh/local-shell)
        │  parse_agent_status()  +  should_apply() seq dedup
        ▼
SessionEvent::AgentStatus(Arc<AgentStatusEvent>)
        │
LocalTerminalView (terminal-view)
        │  agent_status = Some(ev)              ← existing (latest event)
        │  AgentRegistry::apply(key, grouping, &ev, cx)   ← NEW: fold into card
        │  (on Exited/Closed) AgentRegistry::set_lifecycle(Ended)
        ▼
oneterm-state::AgentRegistry  (global Entity, folds → AgentCard, cx.notify)
        │  observed by
        ▼
oneterm-agent-ui::AgentListView / AgentCardView   (groups by tab_key, renders)
        │  hosted by
        ▼
oneterm-app::AgentPanel  (DockItem::Panel "agent_panel", RightDockMode::Agent)
```

The one new write from `terminal-view` is the `AgentRegistry::apply` call in
the same `SessionEvent::AgentStatus` arm that already sets
`LocalTerminalView.agent_status`. Everything downstream is read-only rendering.

---

## 13. Phasing

| Phase | Deliverable |
|---|---|
| **P1** | `AgentRegistry` + `AgentCard` fold in `state`; `terminal-view` pushes events + grouping + lifecycle. |
| **P2** | `agent-ui` `AgentListView` + `AgentCardView`: header/summary, tab groups, card header, state badge/accent, footer, empty state. Replace `app/src/agent_panel.rs` placeholder to compose it. |
| **P3** | Model chip + context-usage bar; running/last tool row; file feed. |
| **P4** | Approval banner (display-only); blocked pinning; risk pills. |
| **P5** | Filters, collapse idle/done, click-to-focus tab+space, stale/ended lifecycle chips. |
| **P6** (opt-in) | Tier-2 approval actions via `send_text`/`send_keystroke`, per-agent opt-in. |

---

## 14. Decisions (resolved)

All previously-open questions are now decided; the choices are folded into the
sections above and restated here for reference.

1. **Space label = numbering.** A card labels its Space `single` (one-Space
   tab) or `#N` — the agent's Space numbered in `SpaceTree` order (depth-first,
   left→right). Numbering is stable and unambiguous under nested splits.
   *Positional* location is conveyed by the hover highlight and the
   active-Space accent, not by the label (§5.1, §7).
2. **Multiple agents per terminal = one card each.** Legal but rare (dedup is
   per `(terminal, agent_id)`). The panel renders one card per
   `(terminal_key, agent_id)`; when a single terminal reports >1 agent, the
   cards stack under the same Space sub-label within that Tab group (§2.1).
3. **Stale threshold = a `UiConfig` key.** Add `agent_stale_threshold_ms` to
   `UiConfig` (persisted in `ui_config.json`), default `300_000` (5 min). The
   registry uses `max(agent_stale_threshold_ms, 3 × heartbeat_interval)` per
   card and exposes the value in the ⚙ menu (§3.1, §8, §9).
4. **Ended-card retention = keep until close + manual clear.** Ended/crashed
   cards drop to the bottom, greyed, and remain until the Tab/Space closes
   (`remove_terminal`) or the user picks "Clear ended". No time-based
   auto-expiry (§9, §8).
5. **Click = activate Tab, or Tab + Space.** Clicking a card activates the
   agent's Tab; for a split Tab it additionally activates the agent's Space via
   `SpaceTree::set_active` and focuses the terminal. Always through OneTerm's
   own dock/focus APIs, never OSC (§7).
