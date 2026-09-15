# Work: OSC handling is the engine's, with a route table an embedder can extend and override

ID: US-0098
Intake: IN-0038
Created: 2026-09-15

## Status

<!-- HARNESS:STATUS:BEGIN -->
- [x] Planned
- [ ] In progress
- [ ] Implemented
- [ ] Changed
- [ ] Reopened (acceptance rework)
- [ ] Retired
<!-- HARNESS:STATUS:END -->

## Classification

- Change type: new capability (plus an existing-contract change to `Config` and `VtEvent`)
- Risk lane: high_risk
- Spec Intake: `IN-0038`

## Outcome

`OscClaims` becomes `OscRoutes`, a per-number table of `Builtin` / `BuiltinAndForward` / `Forward` /
`Drop`. The engine gains built-in handlers for OSC 7, 9 and 133 and emits typed events for them and
for OSC 22 and 50. `crates/terminal/src/osc.rs` loses its whole parsing half and keeps only policy.
OneTerm's OSC 20308 agent channel keeps working through three lines of configuration and no engine
change -- which is the packet's real proof.

## Scope

- [ ] In scope: `crates/vt/src/terminal/osc.rs` (the table), `dispatch.rs` (the route lookup and six
  new arms), `events/vt_event.rs` (seven new variants, `Progress`, `ShellMark`),
  `crates/terminal/src/osc.rs` (delete the parsers), `backend/osc_router.rs` (route typed events),
  `handle.rs` (`adapter_config`), `docs/osc-sequences-checklist.md` (rewrite).
- [ ] Out of scope: OSC 1 (`US-0102` -- it is a conformance gap, not a migration), OSC 17 / 19, OSC
  777, OSC 1337.
- [ ] Out of scope: moving `TerminalSecurityPolicy`, `encode_osc52`, `osc_color.rs`, `url_policy.rs`
  or anything in `osc_agent/`. Decision (f).
- [ ] Out of scope: per-sub-code routing. Considered and rejected in
  [`low-level-design/osc-extension.md`](low-level-design/osc-extension.md).

## Acceptance

Every criterion below is a test a hostile verifier can run and read.

**The table**

- [ ] All six rows of the `forward` / `suppress` / `has_builtin` truth table
  (`osc-extension.md`, "The routing table") are asserted, for a built-in number, a non-built-in
  number below 2048, and a number above 2048.
- [ ] `route(0, Forward)` then `OSC 0;hello ST`: exactly one `VtEvent::Osc { code: 0 }`, zero
  `VtEvent::Title`, and the terminal's title is unchanged.
- [ ] `route(0, BuiltinAndForward)` then the same input: exactly two events, `Title` **then** `Osc`,
  in that order.
- [ ] `route(8, Drop)` then a hyperlink sequence: zero events, no hyperlink on the cell,
  `unhandled_sequences` incremented by exactly 1.
- [ ] `route(20308, Forward).large(20308, true)` then a 3 MiB payload: one `VtEvent::Osc` with
  `truncated == false` and the whole payload readable. Without `large`, the same input yields
  `truncated == true` and at most `OSC_INLINE` bytes.
- [ ] A table routing **every** number to `Forward` turns the engine into a pure parser: a title
  sequence changes no engine state and arrives raw.
- [ ] `route(n, Builtin)` where `has_builtin(n)` is false, and `large(n, true)` where the route is
  `Drop`, each trip a debug assertion and are each inert in release.

**The migration**

- [ ] Every input literal in `crates/terminal/src/osc.rs`'s current `mod tests` appears verbatim in
  the new `crates/vt` test module, with the same expectation. A verifier diffs
  `git show main:crates/terminal/src/osc.rs` against the new tests and checks the list: OSC 7
  `file:///home/marc`, `file://host/path`, a bare path, `%20`, `file:///C:/Users`; OSC 9;4 states 0
  to 4 and the percent clamp; OSC 9 with an embedded `;`; OSC 133 `A`, `B`, `C`, `D`, `D;0`,
  `D;not-a-number`.
- [ ] `crates/terminal/src/osc.rs` no longer contains `parse_osc`, `OscPayload`, `parse_cwd_url`,
  `percent_decode`, `strip_windows_drive_slash` or `Osc133Kind`.
- [ ] `crates/terminal` production line count drops by at least **350** (`git diff --stat`).
- [ ] `adapter_config` in `handle.rs` contains **no** reference to OSC 7, 9 or 133.

**The worked example**

- [ ] The whole OSC 20308 agent channel -- sub-code match, base64, schema validation, `seq` dedup,
  truncation drop, unknown-sub-code counter, support reply -- is unchanged in `crates/terminal/src/
  osc_agent/` and `backend/osc_router.rs`, and `cargo test -p oneterm-terminal` passes its existing
  suite untouched.
- [ ] `grep -rn '20308\|AGENT_OSC' crates/vt/` returns **0** lines.

**No regression**

- [ ] `cargo test --workspace`, `cargo test -p oneterm-vt --features vt-paranoid`, and the 46 frozen
  parity corpus recordings all pass.
- [ ] The extended proptest (arbitrary bytes plus an arbitrary `OscRoutes`) never panics and keeps
  `FeedStats` counters monotone within a batch.
- [ ] A one-hour local fuzz run of `crates/vt/fuzz/fuzz_targets/parser.rs`, with the route table
  derived from the input, finds no crash. Recorded in Evidence; not a CI gate.
- [ ] Manual Windows walk: the SFTP panel follows the shell's cwd (OSC 7); an agent event reaches the
  Agent Panel (OSC 20308); `printf '\033]9;4;1;40\a'` moves the taskbar progress; a prompt mark is
  recorded (OSC 133); a title change still works (OSC 0).

## Documentation

### Owning Docs Reviewed

- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/dispatch-and-modes.md` -- the "OSC
  registration" section this packet replaces. **Stale after this packet.**
- `docs/spec-intakes/IN-0029-vt-engine/low-level-design/events-and-api.md` -- the events-are-values
  rule and the `EventBatch` arena. Confirmed still true; the new variants follow it.
- `docs/osc-sequences-checklist.md` -- its methodology section names the four adapter files as the
  source of truth for OSC. **Stale after this packet**; three of the four no longer parse anything.
- `docs/osc-agent-status.md` -- sections 3.1 (the `9;7` deprecation), 3.2 (the support query), 3.4
  (payload size) and 3.5 (malformed payloads). Its wire contract must not change.
- `docs/terminal-backend.md` -- the `OscRouter` description.
- `crates/terminal/src/security_policy.rs` -- read to confirm every policy call survives the move in
  the same order.

### Documentation Action

**Update required**: `dispatch-and-modes.md` (the OSC registration section becomes a pointer to
`osc-extension.md`), `docs/osc-sequences-checklist.md` (rewritten: which OSC the **engine** handles,
which the adapter routes, and the new `OscRoute` column), `docs/terminal-backend.md` (the
`OscRouter` paragraph). `docs/osc-agent-status.md` does not change: Open Decision 1 keeps the
`9;7` alias, handled in the adapter through the engine's `BuiltinAndForward` route.

Reason: three current documents name adapter files as the place OSC is parsed, and after this packet
that is false in all three.

### Reconciliation

Before completion, list the three or four documents changed.

## Context

The key design is [`low-level-design/osc-extension.md`](low-level-design/osc-extension.md) and must
be read before implementation: it carries the `OscRoute` semantics, the bit layout, the dispatch
path, the exact signatures, the per-parser migration table and the `OSC 9;7` collision analysis.

Sites this packet touches, with line numbers on `main` @ `36977ca`:

| Site | What |
| --- | --- |
| `crates/vt/src/terminal/osc.rs` | the whole file: `OscClaims` -> `OscRoutes` |
| `crates/vt/src/terminal/dispatch.rs:1207-1305` | `Handler::osc`, the built-in match, `forward_osc` |
| `crates/vt/src/events/vt_event.rs:38` | `VtEvent`, seven new variants |
| `crates/terminal/src/osc.rs:84-179` | `parse_osc`, deleted |
| `crates/terminal/src/backend/osc_router.rs:180-231` | the `VtEvent::Osc` arm, rewritten |
| `crates/terminal/src/handle.rs:184-206` | `adapter_config` |

## Plan

- [ ] `OscRoutes` and `OscRoute` in `crates/vt/src/terminal/osc.rs`, with the truth-table tests,
  before anything else uses them.
- [ ] Rewire `Handler::osc` to the route lookup; existing arms keep their behaviour. `cargo test
  --workspace` must be green here, with `crates/terminal` still doing its own parsing, before any
  parser moves. This is the checkpoint that separates "the table works" from "the migration works".
- [ ] New `VtEvent` variants, `Progress`, `ShellMark`.
- [ ] Move the OSC 7 parser (URL, percent decoding, Windows drive slash), with its tests.
- [ ] Move the OSC 9 parsers (progress, notification), with their tests.
- [ ] Move the OSC 133 parser, keeping the existing template-marking behaviour, with its tests.
- [ ] Emit `VtEvent::Pointer` from the OSC 22 arm and `VtEvent::CursorStyleChanged` from OSC 50.
- [ ] Rewrite `OscRouter::handle`'s arms; delete `OscPayload` and `parse_osc`.
- [ ] Rewrite `adapter_config`: `route(20308, Forward).large(20308, true)` and
  `route(9, BuiltinAndForward).large(9, true)`; the adapter discards the `Notification` whose
  forwarded first parameter is `7` and routes that payload to `osc_agent` (Open Decision 1 ruled:
  the alias stays, handled outside the engine).
- [ ] Extend the proptest and the fuzz target with a route table.
- [ ] Rewrite `docs/osc-sequences-checklist.md`; update the two design documents.
- [ ] CHANGELOG lines under `Unreleased`.

## Decisions

- [`DEC-0017`](../../decisions/DEC-0017-osc-routing-table-not-handler-registry.md) -- OSC extension
  is a routing table, not a handler registry. Proposed; the owner accepts. This packet must not start
  before it is accepted, because the alternative shapes it rejects would each be a different packet.

## Verification Plan

- Focused: the table truth-table suite; one test per moved parser, using the adapter's own input
  literals; the override, wrap, suppress and extend tests.
- Unit: `cargo test -p oneterm-vt`, `cargo test -p oneterm-terminal`, `cargo test --workspace`.
- Integration: `cargo test -p oneterm-core -p oneterm-terminal -p oneterm-local-shell -p oneterm-ssh`.
- Property and fuzz: the extended proptest in CI; a one-hour fuzz run recorded, not gated.
- Platform: `pwsh scripts/ci-local.ps1`; the parity corpus replay.
- E2E: the manual Windows walk in Acceptance.

<!-- HARNESS:PROOF:BEGIN -->
- [ ] Unit proof
- [ ] Integration proof
- [ ] E2E proof
- [ ] Platform proof
- [ ] Verify command passed
<!-- HARNESS:PROOF:END -->

## Evidence and Gaps

Record: the truth-table test output; the input-literal diff proving every adapter test case
survived; `git diff --stat` for both crates; the `grep -rn '20308' crates/vt/` result; the fuzz run
duration and result; the manual walk with which OSC each step exercised.

Expected gaps: OSC 1, 17, 19, 777 and 1337 remain unhandled (OSC 1 is `US-0102`'s; the rest are
nobody's yet, deliberately). Whether the `9;7` alias survived is recorded here, with the owner's
ruling on Open Decision 1.

## Handoff

The checkpoint in Plan step 2 -- table landed, nothing moved yet, workspace green -- is the natural
session boundary and the safest place to stop.
