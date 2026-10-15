# High-Level Design: OSC 7501 program status

Intake: IN-0049
Lane: high-risk (persisted settings and dock documents change meaning; a published protocol,
OSC 20308, is withdrawn; a new reply goes out to programs)
Date: 2026-10-09, rewritten 2026-10-15 after the owner's rulings

> The 2026-10-09 version of this file recommended showing 7501 records as cards in the Agent
> Panel, next to OSC 20308. The owner ruled otherwise on 2026-10-15 (see the intake's
> "Owner rulings"): the Agent Panel and OSC 20308 go, and 7501 becomes a status icon in the
> tab title. This file describes that design only.

## Idea

OneTerm stops being an agent dashboard and becomes a terminal that tells you which tab needs
you.

1. **Remove** the Agent Panel (`crates/agent-ui`), its model (`AgentRegistry`, the focuser),
   OneTerm's own agent channel (OSC 20308 and its deprecated `OSC 9;7` alias), the right
   dock's "Agent" mode, the stale-threshold setting, the demo emitters and the docs that
   describe them.
2. **Receive OSC 7501** (Program Status Protocol, rev 0.3; grammar in the intake) in
   `oneterm-terminal`: answer the `7501;?` probe so Claude Code and pi start reporting, keep
   the spec's per-terminal record table, and hand the UI one summary state per batch.
3. **Show it in the tab title**: an icon left of the label, chosen by the highest-priority
   state across the terminal's records. Only for a tab with a single Space.

No engine change, no new dependency, no new setting, nothing persisted.

## Diagram

```text
 program (Claude Code, pi, ...)          local PTY or SSH channel
        |  ESC ] 7501 ; state=working:app=claude ST
        v
 oneterm-vt engine ---- OscRoute::Forward + large (7501) ----> VtEvent::Osc
        |                                                         |
        |  VtEvent::ShellMark(PromptStart)                        v
        +--------------------------------------------> OscRouter (oneterm-terminal)
                                                        |  "?"  -> reply ESC]7501;?ST (byte order)
                                                        |  report -> parse -> RecordTable
                                                        |  133 A  -> drop working/blocked/idle
                                                        v
                                     SessionEvent::ProgramStatus(Option<ProgramState>)
                                       (once per batch, only when the summary changed)
                                                        |
                                                        v
                                      TerminalView.program_status  (+ exit / ack rules)
                                                        |
                                                        v
                         TerminalPanel tab strip: icon left of the label (single Space only)
```

## UI Wireframe

Tab strip, one tab per state (`[x]` is the existing close button):

```text
 | (o) claude        [x] |  working  - loader-circle, rotating, accent colour
 | (!) claude        [x] |  blocked  - bell, pulsing opacity, warning colour
 | (v) build         [x] |  done     - circle-check, success colour (static)
 | (x) tests         [x] |  error    - circle-x, danger colour (static)
 |     pwsh          [x] |  idle / no record - no icon, label unchanged
 | [A] (o) ssh:web   [x] |  channel chips stay left of the icon
 | |B| pwsh          [x] |  split tab (two Spaces): no icon, whatever the terminals report
```

Title bar right-dock control before and after:

```text
 before: [ SSH Client | Agent | None ]
 after:  [ SSH Client | None ]
```

The right dock's "Agent" panel disappears; nothing replaces it.

## Data Flow

1. The engine routes OSC 7501 out unparsed (`Forward`, `large`, so the spec's 4096-byte
   ceiling is not cut at the 2 KiB inline bound). The router drops a truncated sequence.
2. `?` alone is the probe. The reply `ESC ] 7501 ; ? <same terminator>` is written in byte
   order with the batch's other replies, so a program sending `7501;?` then `CSI c` sees
   OneTerm's answer before DA1.
3. Anything else is parsed by the rev 0.3 rules (the intake's grammar and limits). A valid
   report replaces or clears records in the session's `RecordTable` (cap 64, least recently
   updated evicted).
4. OSC 133 `A` (prompt start) drops `working`, `blocked` and `idle` records; `done` and
   `error` stay.
5. After each batch, if the summary changed, the router emits
   `SessionEvent::ProgramStatus(summary)`. Summary priority:
   `blocked > error > working > done > idle`; no record means `None`.
6. `TerminalView` stores the summary. On process exit it drops `working`, `blocked` and
   `idle`. It clears `done` / `error` once acknowledged: the tab is selected, or, if it is
   already selected, the user types into the terminal.
7. `TerminalPanel` draws the icon when its tree has a single Space; the tab repaints because
   the view notifies.

### Owner rulings this design applies

| Question | Ruling |
| --- | --- |
| Icon set | working: rotating `loader-circle`; blocked: pulsing `bell`; done: `circle-check`; error: `circle-x`; idle and no record: none. Theme colours. |
| When shown | Every tab, the selected one included. `done` / `error` clear once acknowledged. |
| Saved `right_dock_mode: "agent"` | Read as `none` (dock hidden). |
| Several records | Highest priority across all records of the terminal. |
| Split tab | No icon. |

### Deliberate deviations from rev 0.3

- **`RIS` does not clear records.** The engine reports `RIS`, `ED 2` and `ED 3` as one
  `ScreenCleared` event; separating them is a public `oneterm-vt` API change for one rarely
  used rule. A `RIS` mid-session leaves the last state until the next prompt or a report.
- **`msg` and `title` are validated, never shown.** The spec's security rules are applied in
  full (a report with bad base64 or a control character is discarded), but the text is not
  kept: the icon needs only the state. Nothing in OneTerm's chrome repeats remote text.
- **No OSC 9;4 mapping.** The progress bar stays a separate surface.

## Detail Design

- [x] Detail design: required (high-risk)
  - [`low-level-design/agent-panel-removal.md`](low-level-design/agent-panel-removal.md):
    what is deleted, and how saved settings and dock layouts still load.
  - [`low-level-design/osc-7501-receiver.md`](low-level-design/osc-7501-receiver.md):
    parser, record table, router wiring, summary event.
  - [`low-level-design/tab-status-icon.md`](low-level-design/tab-status-icon.md): view
    state, acknowledgement, the icon element.
