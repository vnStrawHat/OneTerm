# Work: Move the agent-status protocol off OSC 9;7 to OSC 20308

ID: US-0088
Intake: IN-0029
Created: 2026-09-13

> Pre-code gate: complete Outcome, Scope, Acceptance, Documentation, and Verification Plan before editing implementation files. Harness synchronizes only the marked status/proof blocks; keep authored checklists current.

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [ ] Planned
- [ ] In progress
- [x] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: existing-contract change (a published wire protocol moves number)
- Risk lane: high_risk — `docs/osc-agent-status.md` is a **public contract**: it asks
  third-party agents to write these bytes, and the number being left behind is
  ConEmu's "run some process with arguments".
- Spec Intake, when required: IN-0029 (`docs/spec-intakes/IN-0029-vt-engine/IN-0029.md`, US-0088)

## Outcome

`OSC 20308 ; 1 ; <base64-json>` is the agent-status sequence and reaches exactly the
same `SessionEvent::AgentStatus` handling `OSC 9 ; 7` reached — byte-for-byte the same
payload grammar, the same 8 KiB cap, the same schema validation, the same `seq` dedup,
the same silent-drop policy. `OSC 20308 ; 0` answers the support query
(`ESC ] 20308 ; 0 ; 1 ; OneTerm ; <version> ST`) on the transport, under the engine
guard, before the batch yields (R-37). `OSC 9 ; 7` still works for one release, is
marked deprecated and logs once per session at `debug`, with no user-visible change.
An unknown `OSC 20308 ; <n>` sub-code is ignored and counted.

## Scope

- [x] In scope:
  - `crates/vt` — the **claim** only: 20308 is above the 2048 bitmap, so it exercises
    the `high` list. No engine behaviour change; the registration table already routes
    an arbitrary `u32`, which is the extension point `US-0086` hardened.
  - `crates/terminal` — the claim in `handle.rs`, the sub-code match in `osc.rs`, the
    support-query reply and the two counters in `backend/osc_router.rs` +
    `backend/state.rs`, the test-support encoder in `osc_agent/`.
  - The two reference emitters, `scripts/agent-status-demo.{sh,ps1}`, which today ask
    ConEmu to spawn a process.
  - Docs: `docs/osc-agent-status.md` (finish the dated sections),
    `docs/osc-sequences-checklist.md` Group G, `README.md`, `scripts/README.md`,
    `docs/README.md`, `docs/PROJECT.md`, `AGENTS.md`, and the crate/module doc
    comments that name the old number.
  - Tests: the existing agent tests **parametrised** over both encodings, plus the two
    rows `dispatch-and-modes.md` reserves.
- [x] Out of scope:
  - The agent-panel consumer (`crates/agent-ui`, `crates/state`,
    `crates/terminal-view`). `SessionEvent::AgentStatus(Arc<AgentStatusEvent>)` is
    unchanged, so nothing above the router can tell the two encodings apart — which is
    the point. Only prose in their doc comments changes.
  - Dropping `9;7`. The spec gives it one release; the deletion is the next one.
  - The payload schema, the state machine, the redaction rules, the security limits.
    Only the two leading parameters move.
  - `crates/completion/assets/` — see Documentation Action.

## Acceptance

- [x] `OSC 20308 ; 1 ; <b64>` produces the same `SessionEvent::AgentStatus` as
      `OSC 9 ; 7 ; <b64>` for the same payload, through the same `parse_agent_status`.
- [x] `OSC 20308 ; 0` writes `ESC ] 20308 ; 0 ; 1 ; OneTerm ; <version>` to the
      transport, terminated the way the query was terminated, and emits no UI event.
- [x] `OSC 9 ; 7` still parses and routes; the deprecation is logged at most once per
      session and nothing user-visible changes.
- [x] `OSC 20308 ; <n>` for any other `n` (and `OSC 20308` with no sub-code) yields no
      event and increments a counter readable from `SharedSessionState`.
- [x] Every pre-existing agent test passes for both encodings, parametrised rather
      than duplicated.
- [x] `python scripts/completion-catalog.py validate` green.
- [x] `docs/osc-sequences-checklist.md` Group G names 20308 and marks 9;7 deprecated.
- [x] The 45-recording gate and `vt-diff` are unaffected: no recording carries either
      sequence.
- [x] The agent panel shows the same status for both encodings, in the running app.

## Documentation

### Owning Docs Reviewed

- `docs/osc-agent-status.md` — the wire contract. §2.1/§2.2 (why the number changed and
  how 20308 was picked), §3 (wire format), the migration table and the support query
  were written by the design owner ahead of this packet. Two defects left in it: the
  section numbers restart (`3.1`/`3.2` appear twice) and the receiver description still
  says "detects the `7` sub-code".
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` —
  § "OSC registration": `crates/terminal` claims 7, 9, 133, 633 and **20308**, 52 and 8
  large; both codes are sub-code matches in `crates/terminal/src/osc_agent/`, the engine
  only routes numbers; the two reserved test rows. Read, not changed (design owner's).
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` and
  `damage-and-render-state.md` — R-37, the reply-before-yield ordering the support reply
  has to honour.
- `docs/osc-sequences-checklist.md` — Group G row for 9;7.
- `docs/agents/error-policy.md` — "optional telemetry/UI refresh": a malformed or
  unknown sequence logs at `debug` and continues; it is never an error. That is already
  what the receiver does and what the new sub-codes must do.
- `docs/agents/code-style.md` — no new crate, no abstraction for one caller.

### Documentation Action

Update required:

- `docs/osc-agent-status.md` — finish it: renumber the duplicated §3.1/§3.2 and correct
  the stale "the `7` sub-code" sentence, which describes the receiver this packet
  replaces.
- `docs/osc-sequences-checklist.md` — Group G gains a `20308` row and the `9;7` row
  becomes deprecated.
- `README.md`, `scripts/README.md`, `docs/README.md`, `docs/PROJECT.md`, `AGENTS.md` —
  each names "the OSC 9;7 proposal" in prose.
- `scripts/agent-status-demo.{sh,ps1}` and the crate/module doc comments in
  `crates/{terminal,terminal-view,state,agent-ui,settings,tools}`.

No contract change, with the reason recorded rather than the file edited:

- `crates/completion/assets/` — the intake's touch list names the completion catalogs,
  but a scoped grep for `9;7` across `crates/completion/` and every `*.json`/`*.toml`
  in the tree returns **nothing**: the catalogs describe shell commands, not OSC
  sequences, and no catalog entry mentions the agent channel. Nothing to update. The
  validator is still run, as acceptance requires.

### Reconciliation

Docs changed: `docs/osc-agent-status.md`, `docs/osc-sequences-checklist.md`,
`README.md`, `scripts/README.md`, `docs/README.md`, `docs/PROJECT.md`, `AGENTS.md`.
The `crates/completion/assets/` no-change reason above was re-checked after
implementation and still holds.

## Context

- The registration table (`crates/vt/src/terminal/osc.rs`) already stores codes above
  2048 in a sorted `Vec`, and `parse_code` accepts any `u32`. 20308 therefore needs
  **zero** engine change — claiming it is the whole engine-side story, which is what
  `dispatch-and-modes.md` predicted the extension point would buy.
- `parse_osc` is a pure function over the raw parameter slices. The support query has
  to *write bytes*, so it travels as a new `OscPayload` variant and the router — which
  already owns `reply()` and the transport — answers it.
- R-37: `OscRouter::drain` writes every `VtEvent::Reply` in a first pass, then routes
  the rest. The support reply is formatted by the embedder, so it lands in the second
  pass — still under the engine guard and still before the pump's yield check and
  before any UI event is flushed, which is what R-37 protects.
- "Counted" needed a home: `crates/terminal` has no drop counters, only the engine's
  `FeedStats`, and the engine cannot count a sub-code of a number it forwards. One
  atomic on `SharedSessionState` (already `Arc`-shared, already the home of the agent
  `seq` watermarks) is the smallest thing that makes the drop observable and testable.

## Plan

- [x] Work packet + harness row before any code.
- [x] `crates/vt`: claim-side test rows and the module doc; no engine edit.
- [x] `crates/terminal`: claim, sub-code match, support reply, counters.
- [x] Parametrise the existing agent tests over both encodings.
- [x] Emitters and docs.
- [x] `pwsh scripts/ci-local.ps1`.

## Decisions

No new decision record. The number, the band it was chosen from and the one-release
alias are already decided and written down in `docs/osc-agent-status.md` § 2.2 and
§ 3.1 by the design owner; this packet implements that, it does not choose it.

## Verification Plan

- Unit: `crates/terminal/src/osc.rs` tests — both encodings parse to the same
  `OscPayload::AgentStatus`; the support query; unknown sub-codes; `9;4` progress and
  `9;<msg>` notifications still route the old way (the alias must not eat OSC 9).
- Unit: `crates/vt` — `osc_20308_reaches_the_embedder_through_a_claim` and
  `osc_9_7_still_reaches_the_embedder_during_the_alias_release`, the two rows
  `dispatch-and-modes.md` reserves.
- Integration: `crates/terminal/src/backend/backend_tests.rs` — the router tests
  (forwarding, `seq` dedup) parametrised over both encodings, plus the support-reply
  bytes read off `FakePtyTransport` and the unknown-sub-code counter.
- Regression: the 45-recording parity gate and `vt-diff` — confirmed unaffected by a
  byte scan of `crates/vt/tests/corpus` for `ESC ] 9 ; 7` and `ESC ] 20308` (and their
  C1 `0x9d` forms) before any edit: 231 files, 0 hits.
- `python scripts/completion-catalog.py validate`.
- `pwsh scripts/ci-local.ps1` (the full CI gate).
- E2E, only if the desktop is Active: one throwaway instance built from this
  worktree, emitting both spellings, screenshotted under a `US-0088-` prefix. The
  owner's own instance is never enumerated, driven or stopped.

<!-- HARNESS:PROOF:BEGIN -->
- [x] Unit proof
- [x] Integration proof
- [x] E2E proof
- [ ] Platform proof
- [x] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

### Results

- `pwsh scripts/ci-local.ps1` — **green, exit 0**, all ten steps.
- Raw totals over the two test steps: **62 sections, 1939 passed / 0 failed / 15
  ignored** — `cargo test --workspace` 58 sections 1565/0/12 and
  `cargo test -p oneterm-vt --features vt-paranoid` 4 sections 374/0/3. `US-0086`
  recorded 62/1927/0/15, so the delta is **+12 tests, none removed**: 5 in
  `crates/terminal/src/osc.rs`, 4 in `backend_tests.rs`, 1 in `crates/vt`
  (`osc_9_7_reaches_the_embedder_through_a_claim` became two), each counted twice
  where a suite runs in both the plain and the `vt-paranoid` step.
- Recording gate: `vt-corpus check --engine new` — **45 recordings, 45 passed, 0
  failed**. `vt-diff` — **45 recordings, 45 identical, 0 differing**.
  `vt-corpus grep-deviations` reports nothing for either code. Unchanged, as the
  pre-edit byte scan predicted: neither sequence occurs in any recording.
- `python scripts/completion-catalog.py validate` — `all catalogs valid` (inside
  `ci-local.ps1`).

### What routes where

| Bytes | Result |
|---|---|
| `ESC ] 20308 ; 1 ; <b64> ST` | `parse_agent_status` → `OscPayload::AgentStatus` → `seq` dedup → `SessionEvent::AgentStatus` |
| `ESC ] 9 ; 7 ; <b64> ST` | identical, plus one `debug` line per session |
| `ESC ] 20308 ; 0 ST` | `ESC ] 20308 ; 0 ; 1 ; OneTerm ; <version> ST` written to the transport; no UI event |
| `ESC ] 20308 ; <n> ST`, `ESC ] 20308 ST` | dropped, `SharedSessionState::agent_osc_unknown_subcodes()` +1 |
| `ESC ] 9 ; 4 ; …`, `ESC ] 9 ; <msg>` | unchanged — progress and notification |

### GUI check — run, and it is the acceptance evidence

`quser` reported the session **Active**, so the check was run.
`evidence/US-0088-agent-panel-both-encodings.png`.

Driven **without keystroke injection**, which is what made it safe: a throwaway
instance was built and launched from this worktree, and because `config_dir()` is
`target/` in a debug build, it read `<worktree>/target/{terminal.json,ui_config.json}`
— written for the run and deleted after — rather than anything the owner's instance
uses. Those files set the right dock to `agent` and the shell to a scratchpad script
that emits both spellings of the *same* payload as two agent ids. So no window was
focused, enumerated or typed into.

What the screenshot shows:

| | `new-20308` (via `20308;1`) | `legacy-9-7` (via `9;7`) |
|---|---|---|
| state | `working` | `working` |
| model | anthropic > Claude Opus 5 | anthropic > Claude Opus 5 |
| context | 42k / 200k, 21% | 42k / 200k, 21% |
| session | `sid s-new` | `sid s-old` |

Identical in every field except the two the emitter deliberately varied. The panel
header reads `All 2 · Work 2 · Block 0 · Err 0 · Idle 0 · Done 0` — so the reserved
`OSC 20308;4` sent between them produced **no card**, and the `OSC 20308;0` support
reply left through the PTY and was consumed by the shell rather than echoed as stray
text, which is visible as the clean line after `sent OSC 20308;0`.

**The owner's `oneterm.exe` was never enumerated, driven or stopped.** The two
pre-existing processes (the owner's `dist` build at pid 27376 and another agent's
worktree build at 31376) were recorded before launch, only the launched pid 31448 was
stopped, and both were confirmed alive afterwards.

### Gaps

1. **No platform proof.** Windows only; the change is platform-neutral (parameter
   matching and one `format!`), but it was not exercised on Linux or macOS.
2. **The alias deletion is not scheduled in code.** `docs/osc-agent-status.md` § 3.1
   says `9;7` is dropped in the release after this one; there is no compile-time or
   test-time reminder that will fire then. The `crates/vt` test
   `osc_9_7_still_reaches_the_embedder_during_the_alias_release` is named for it and
   `dispatch-and-modes.md` reserves "its counterpart asserting the claim is gone in the
   release after", but the counterpart cannot be written until the decision to cut is
   taken.
3. **No third-party emitter has been ported.** Only OneTerm's own two demo scripts
   emit the sequence today. Any agent that already shipped `9;7` keeps working for one
   release; nothing in this repo can verify that they move.
4. **The intake's touch list names the completion catalogs; there was nothing there.**
   Recorded under Documentation Action rather than silently skipped.
5. **Two `9;7` byte literals left in the tree on purpose**, both outside this packet's
   scope and neither reaching a terminal:
   - `crates/tools/src/bench.rs` — the throughput fixture keeps the **name**
     `osc_9_7` because it is the key `US-0072`, `US-0073` and `US-0076` baseline
     tables compare against, and what it measures is OSC-payload throughput, which
     the number does not change. Its stale description was corrected and the reason
     is a comment on the function.
   - `crates/terminal/tests/us0081_parity.rs` — the `osc9-agent` case diffs
     `TerminalContent` (grid state) between the fork and the new engine, where an OSC
     changes nothing either way; `9;7` is still a live alias, so the case still holds,
     and the file retires with the fork at `US-0087`.

## Handoff

State: implemented on `worktree-agent-ad372757f16ebe64c` off `feat/vt-engine`, **not
merged and not pushed**. Next action: the design owner reviews the two doc fixes in
`docs/osc-agent-status.md` (the duplicated section numbers and the stale "`7` sub-code"
sentence), then merges. The release after this one closes gap 2 by deleting the alias.
