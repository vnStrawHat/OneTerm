# High-Level Design: OSC 7501 program status

Intake: IN-0049
Lane: normal (`US-0153`, if accepted, is high-risk: `oneterm-vt` public API)
Date: 2026-10-09

## Idea

OSC 7501 (Program Status Protocol, rev 0.3) lets any program say "I am working / blocked on
you / done / failed". Claude Code 2.1.295 and pi 1.1.0 already speak it. Claude Code probes
first (`OSC 7501 ; ?`) and stays silent when nothing answers, so today OneTerm gets nothing
from it.

OneTerm consumes it **as it consumes OSC 20308**: the engine routes the number out unparsed
(`OscRoute::Forward`, `large`), and a small parser plus a per-session record table in
`oneterm-terminal` apply the spec's rules. The router answers the probe in byte order and
hands the UI one coalesced snapshot per batch. The Agent Panel shows each terminal's records
as a card with the same five states it already draws.

There is no engine change, no new dependency, no persistence and no setting. The single
thing the engine cannot tell the adapter today, `RIS` as opposed to `ED 2`, is an optional
additive event (`US-0153`).

## Recommendation

**Support, partially: receiver and Agent Panel only (`US-0152`); everything else optional.**

- *For:* real emitters exist, including the agent the owner runs inside OneTerm. Users get it
  for free (no hook, no plugin). Remote agents work over SSH unchanged. The states map one to
  one onto `AgentState`. The work reuses the 20308 seam (route table, byte-ordered replies,
  card registry). The engine and its public API stay untouched.
- *Against, and how it is contained:* the spec is three days old at rev 0.3, so the parser
  is pinned to rev 0.3 in one module and unknown keys are ignored as the spec requires. It
  is a second channel next to 20308, contained by one source of truth per terminal (Q4).
  Untrusted remote text is shown in OneTerm's chrome, contained by refusing control
  characters, stripping invisible characters, labelling the source tab, and never acting on a
  record.
- *Not now:* a built-in `VtEvent::ProgramStatus` in `oneterm-vt` (Q2); OSC 9;4 mapping (Q5);
  notifications (`US-0155`, opt-in later); a `Pst` terminfo entry (OneTerm ships no terminfo
  of its own).

The ConPTY ordering risk (intake Q8) is the one thing that could turn "support" into
"support over SSH only" on Windows. `US-0152` checks it before building anything else.

## Diagram

```text
 program (local ConPTY or remote over SSH)
   | ESC]7501;state=blocked:kind=permission:app=claude-code:msg=<b64> ST
   | ESC]7501;? ST  ESC[c
   v
 oneterm-vt Terminal::feed  (OscRoutes: 7501 -> Forward, large; engine knows nothing else)
   |  VtEvent::Osc { code: 7501, params, terminator, truncated }   (byte order)
   |  VtEvent::Reply(DA1) ...  VtEvent::ShellMark(PromptStart) ... [VtEvent::FullReset: US-0153]
   v
 oneterm-terminal OscRouter::drain   (engine lock held, nothing waits)
   |-- params == ["7501","?"]      -> transport.pty_write("ESC]7501;?" + same terminator)  (before DA1)
   |-- report                      -> program_status::parse (rev 0.3 grammar, limits, base64, controls)
   |                                  -> ProgramRecords (per session, cap 64, LRU, subtree clear)
   |-- ShellMark::PromptStart      -> ProgramRecords::on_prompt()  (drop working/blocked/idle)
   |-- end of batch, table dirty   -> out.push(SessionEvent::ProgramStatus(Arc<[ProgramRecord]>))
   '-- session exit (pump)         -> ProgramRecords::on_exit()    (same drop rule)
   v
 terminal-view agent_status.rs  push_program_status(snapshot)   (+ Grouping, nav, as for 20308)
   v
 oneterm-state AgentRegistry::replace_program_records(terminal_key, grouping, snapshot)
   |   card key (terminal, "7501") ; stale timer off for these cards
   v
 agent-ui view   (existing card list, filters, sort, click-to-focus)
```

## UI Wireframe

One card per terminal for 7501. The headline is the root record, or the most urgent record
when there is no root (blocked > error > working > done > idle). Up to three child rows
follow, then `+N more`. The card sits in the existing tab group, sorted by the existing rank.

```text
+- Agents ----------------------------------------+
| [# All] [* Work] [^ Block] [x Err] [. Done]     |
+-------------------------------------------------+
| pwsh - myTerm2                            * 1   |
| +- card ------------------------------------+   |
| | ^ blocked  permission     claude-code #0  |   |   state + kind | app + space
| | Allow Bash: cargo test --workspace?       |   |   msg (decoded, one line, ellipsised)
| |   build/test     * working   [=====   ] 62%|   |   child rows: id, state, progress
| |   deploy/eu-west ^ blocked   permission   |   |
| |   +2 more                                 |   |
| | 12s ago                    OSC 7501       |   |   age | source protocol
| +-------------------------------------------+   |
+-------------------------------------------------+
```

- No model row, context bar or tool row (7501 carries none). No `stale` liveness label
  (the protocol forbids it). `title`, when present, replaces `app` as the label.
- `kind=auth` reads "needs authentication in this terminal" and offers **no** input field.
  Clicking focuses the terminal, as every card does today.
- The empty-state footnote (`EMPTY_STATE` in `crates/agent-ui/src/view.rs`, held by a unit
  test) changes from naming OSC 20308 only to naming both sequences.
- Theme tokens only; the existing state colours and spinner.

## Data Flow

1. `adapter_config` adds `routes.route(7501, OscRoute::Forward).large(7501, true)`. Without
   `large`, every sequence over about 2 KiB is cut and arrives `truncated: true`.
2. `OscRouter::handle` on `VtEvent::Osc { code: 7501, .. }`:
   - `truncated` -> drop and count (the 20308 rule: a cut payload lies);
   - exactly `["7501", "?"]` -> write the echo reply now, in byte order, with the terminator
     the query used;
   - otherwise -> `program_status::parse(params)`. Reject if more than one parameter follows
     the number, or the sequence exceeds 4096 bytes. Split on `:`, then each pair on the first
     `=`; skip malformed pairs, ignore unknown keys, last value wins. Require a known `state`.
     Validate `id` (a bad id drops the report). Decode `msg` / `title` (bad base64, a decoded
     limit broken, invalid UTF-8 or **any** control character drops the report), then strip
     bidi and invisible formatting characters.
3. `ProgramRecords::apply(report)`: `clear` removes the id's subtree, or everything when there
   is no id. Otherwise the report replaces the record (omitted keys dropped), `app` is resolved
   from the nearest ancestor at read time, and the least recently updated record is evicted
   past 64. Each change marks the table dirty.
4. `ShellMark::PromptStart` and session exit remove `working`, `blocked` and `idle` records
   (Q7) and keep `done` / `error`. `RIS` clears everything once `US-0153` exists; until then
   it is a recorded deviation.
5. After the batch, a dirty table pushes **one** `SessionEvent::ProgramStatus(snapshot)`. A
   1,000-report flood therefore costs one UI event per batch, not 1,000.
6. `TerminalView` forwards the snapshot with its `Grouping` and nav target.
   `AgentRegistry::replace_program_records` swaps the terminal's 7501 card (it removes the
   card when the snapshot is empty) and never marks it stale. With Q4 as recommended, the
   card is hidden while the same terminal has a non-ended 20308 card.

### Engine contract

| Option | `oneterm-vt` change | Public API | Verdict |
| --- | --- | --- | --- |
| A. `Forward` route, parse in the adapter | none | unchanged | **recommended** (`US-0152`) |
| B. A. plus a `RIS`-only event | an additive `VtEvent` variant (enum is `#[non_exhaustive]`, so not breaking), emitted next to `ScreenCleared` from `reset_state` (the `RIS` path in `dispatch.rs`) | +1 variant line in `public-api.windows.txt` and `public-api.unix.txt`; CHANGELOG entry; rustdoc must cite no work-item ids or repository paths | optional (`US-0153`) |
| C. Built-in 7501: `OscRoutes::BUILTIN` grows, a typed `VtEvent::ProgramStatus { .. }` with spans, query answered by the engine behind a `Config` flag (default off, as libghostty-vt only answers while the effect is set) | new parser module, new types | several new items; a contract to keep stable while the spec is at 0.x | deferred (Q2) |

Default under A: on in OneTerm (the route is OneTerm's), and nothing changes for other
embedders of `oneterm-vt`.

### Proposed rows for `docs/osc-sequences-checklist.md` (not applied)

Group G table, after the `20308;0` row:

```markdown
| ☑ | **7501** | Program status report (Program Status Protocol rev 0.3) | `ESC]7501;state=working:app=x:msg=<b64> ST` | ✅ — route `Forward`; records → Agent Panel (`US-0152`). |
| ☑ | **7501;?** | Program-status support query | `ESC]7501;? ST` → `ESC]7501;? ST` | ✅ — answered in byte order, before a following DA1. |
```

Group G note, appended:

```markdown
> **OSC 7501** is the Program Status Protocol
> (<https://www.superlogical.com/rex/docs/build/program-status>, rev 0.3), emitted by
> Claude Code 2.1.295+ and pi 1.1.0+. Like 20308 it is routed **`Forward`** with
> `OscRoutes::large(7501, true)` (the spec allows 4096-byte sequences), and parsed in
> `crates/terminal` into a per-session record table (64 records, least recently updated
> evicted). Reports whose `msg` / `title` decode to a control character are refused whole.
> `RIS` does not yet clear the records (`US-0153`).
```

Quick-summary table row: `| 7501 program status | ✅ | OK (receiver + Agent Panel; no 9;4 mapping) |`.

## Detail Design

- [x] Detail design: not needed for `US-0152` (normal lane; the grammar table in the intake
  and the data flow above are the design). **Required before `US-0153`** if the owner accepts
  it: a low-level design for the variant name, its rustdoc, and the snapshot diff.
- Reason: `US-0152` changes only workspace-internal crates; `US-0153` changes the
  `oneterm-vt` external contract.
